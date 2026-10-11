// Use the session FileChooser portal without changing Chromium's D-Bus build.
#include "carbonyl/src/browser/portal_file_dialog.h"

#include <unistd.h>
#include <fcntl.h>
#include "base/command_line.h"
#include "base/files/scoped_file.h"
#include "base/functional/bind.h"
#include "base/json/json_reader.h"
#include "base/json/json_writer.h"
#include "base/logging.h"
#include "base/path_service.h"
#include "base/posix/eintr_wrapper.h"
#include "base/process/launch.h"
#include "base/process/process.h"
#include "base/strings/utf_string_conversions.h"
#include "base/task/thread_pool.h"
#include "base/values.h"
#include "net/base/filename_util.h"
#include "ui/shell_dialogs/select_file_dialog.h"
#include "ui/shell_dialogs/select_file_dialog_factory.h"
#include "ui/shell_dialogs/select_file_policy.h"
#include "ui/shell_dialogs/selected_file_info.h"

namespace carbonyl {
namespace {
std::string ReadResult(base::ScopedFD pipe, base::Process process) {
  std::string output;
  char buffer[4096];
  ssize_t count;
  while ((count = HANDLE_EINTR(read(pipe.get(), buffer, sizeof(buffer)))) > 0) {
    output.append(buffer, count);
    if (output.size() > 1024 * 1024) {
      process.Terminate(1, false);
      output.clear();
      break;
    }
  }
  int status = 1;
  if (!process.WaitForExit(&status) || status != 0) return {};
  return output;
}
class PortalDialog : public ui::SelectFileDialog {
 public:
  PortalDialog(Listener* listener, std::unique_ptr<ui::SelectFilePolicy> policy)
      : SelectFileDialog(listener, std::move(policy)) {}
  bool IsRunning(gfx::NativeWindow owner) const override {
    return process_.IsValid() && owner == owner_;
  }
  void ListenerDestroyed() override {
    listener_ = nullptr;
    if (process_.IsValid()) process_.Terminate(1, false);
  }
 protected:
  ~PortalDialog() override = default;
  bool HasMultipleFileTypeChoicesImpl() override { return filters_ > 1; }
  void SelectFileImpl(Type type, const std::u16string& title,
      const base::FilePath& path, const FileTypeInfo* types, int index,
      const base::FilePath::StringType& extension, gfx::NativeWindow owner,
      const GURL* caller) override {
    type_ = type;
    owner_ = owner;
    index_ = index;
    base::DictValue request;
    request.Set("save", type == SELECT_SAVEAS_FILE);
    request.Set("directory", type == SELECT_FOLDER || type == SELECT_UPLOAD_FOLDER ||
                             type == SELECT_EXISTING_FOLDER);
    request.Set("multiple", type == SELECT_OPEN_MULTI_FILE);
    request.Set("title", title.empty() ? (type == SELECT_SAVEAS_FILE ?
        "Zapisz plik" : "Wybierz plik") : base::UTF16ToUTF8(title));
    request.Set("path", path.value());
    request.Set("default_extension", extension);
    request.Set("filter_index", index);
    base::ListValue filters;
    if (types) {
      for (size_t n = 0; n < types->extensions.size(); ++n) {
        base::ListValue patterns;
        std::string label;
        for (const auto& ext : types->extensions[n]) {
          std::string pattern = "*." + ext;
          if (!label.empty()) label += ", ";
          label += pattern;
          base::ListValue pair;
          pair.Append(0); pair.Append(pattern);
          patterns.Append(std::move(pair));
        }
        if (n < types->extension_description_overrides.size())
          label = base::UTF16ToUTF8(types->extension_description_overrides[n]);
        base::ListValue filter;
        filter.Append(label); filter.Append(std::move(patterns));
        filters.Append(std::move(filter));
      }
      if (types->include_all_files) {
        base::ListValue pair; pair.Append(0); pair.Append("*");
        base::ListValue patterns; patterns.Append(std::move(pair));
        base::ListValue filter; filter.Append("Wszystkie pliki");
        filter.Append(std::move(patterns)); filters.Append(std::move(filter));
      }
    }
    filters_ = filters.size();
    request.Set("filters", std::move(filters));
    base::FilePath executable;
    base::PathService::Get(base::FILE_EXE, &executable);
    base::CommandLine command(base::FilePath("/usr/bin/python3"));
    command.AppendArgPath(executable.DirName().Append("carbonyl-file-portal.py"));
    command.AppendArg(base::WriteJson(request).value_or("{}"));
    command.AppendArg(std::to_string(getpid()));
    int descriptors[2];
    if (pipe2(descriptors, O_CLOEXEC) != 0) { Finish({}); return; }
    base::ScopedFD read_end(descriptors[0]), write_end(descriptors[1]);
    base::LaunchOptions options;
    options.fds_to_remap.emplace_back(write_end.get(), STDOUT_FILENO);
    process_ = base::LaunchProcess(command, options);
    if (!process_.IsValid()) { Finish({}); return; }
    write_end.reset();
    base::ThreadPool::PostTaskAndReplyWithResult(FROM_HERE,
        {base::MayBlock(), base::WithBaseSyncPrimitives(), base::TaskPriority::USER_VISIBLE,
         base::TaskShutdownBehavior::CONTINUE_ON_SHUTDOWN},
        base::BindOnce(&ReadResult, std::move(read_end), process_.Duplicate()),
        base::BindOnce(&PortalDialog::Finish, base::WrapRefCounted(this)));
  }
 private:
  void Finish(std::string output) {
    process_.Close();
    if (!listener_) return;
    auto result = base::JSONReader::ReadDict(output, base::JSON_PARSE_RFC);
    std::vector<ui::SelectedFileInfo> files;
    if (result && result->FindInt("response").value_or(2) == 0) {
      if (auto* uris = result->FindList("uris")) {
        for (const auto& uri : *uris) {
          base::FilePath path;
          if (!uri.is_string() || !net::FileURLToFilePath(GURL(uri.GetString()), &path) ||
              !path.IsAbsolute()) { files.clear(); break; }
          files.emplace_back(path);
        }
      }
      index_ = result->FindInt("filter_index").value_or(index_);
    }
    if (files.empty()) listener_->FileSelectionCanceled();
    else if (type_ == SELECT_OPEN_MULTI_FILE) listener_->MultiFilesSelected(files);
    else listener_->FileSelected(files.front(), index_);
  }
  Type type_ = SELECT_NONE;
  gfx::NativeWindow owner_ = nullptr;
  int index_ = 0;
  size_t filters_ = 0;
  base::Process process_;
};
class PortalFactory : public ui::SelectFileDialogFactory {
 public:
  ui::SelectFileDialog* Create(ui::SelectFileDialog::Listener* listener,
      std::unique_ptr<ui::SelectFilePolicy> policy) override {
    return new PortalDialog(listener, std::move(policy));
  }
};
}  // namespace
void InstallPortalFileDialog() {
  ui::SelectFileDialog::SetFactory(std::make_unique<PortalFactory>());
}
}  // namespace carbonyl
