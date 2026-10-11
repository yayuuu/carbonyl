#!/usr/bin/env python3
"""Port the original Carbonyl graphics/text bridge to official Chromium 154.

Run once on an unmodified 154.0.8037.97 checkout after prepare-port.py.
The Chromium/Skia diffs are saved separately for reproducibility.
"""
from pathlib import Path
import difflib
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / 'chromium/src'
REF = ROOT / 'build/port-reference/chromium'

def replace(path, before, after, count=1):
    p = SRC / path
    text = p.read_text()
    if len(after) > len(before) and after in text:
        return
    assert before in text, (path, before[:90])
    p.write_text(text.replace(before, after, count))

def resolve(path):
    text = (REF / 'merged' / path).read_text()
    def conflict(match):
        current, base, patched = match.groups()
        if path == 'cc/trees/layer_tree_host.cc':
            return current.replace('  return result;', '  StartTerminalRender();\n  return result;')
        if path == 'skia/BUILD.gn':
            return current.replace('deps = []', 'deps = [ "//carbonyl/src/browser:bridge" ]')
        if path == 'content/browser/renderer_host/render_frame_host_impl.h':
            return current + '  std::unique_ptr<carbonyl::CarbonylRenderServiceImpl> carbonyl_render_service_;\n'
        if path == 'content/browser/browser_interface_binders.cc':
            return current + '''\n  map->Add<carbonyl::mojom::CarbonylRenderService>(
      &BindRenderFrameHostImpl<&RenderFrameHostImpl::GetCarbonylRenderService>);\n'''
        if path == 'components/viz/host/host_display_client.h':
            return '''  protected:
  void CreateLayeredWindowUpdater(
      mojo::PendingReceiver<mojom::LayeredWindowUpdater> receiver) override;
\n''' + current
        if not base.strip():
            return current + patched
        # Retain current APIs, add the unchanged Carbonyl insertions.
        base_lines, patched_lines = base.splitlines(True), patched.splitlines(True)
        additions = ''.join(''.join(patched_lines[j:k]) for op, a, b, j, k in
            difflib.SequenceMatcher(None, base_lines, patched_lines).get_opcodes() if op == 'insert')
        return current + additions
    text = re.sub(r'^<<<<<<< [^\n]*\n(.*?)^\|\|\|\|\|\|\| [^\n]*\n(.*?)^=======\n(.*?)^>>>>>>> [^\n]*\n',
                  conflict, text, flags=re.M | re.S)
    assert '<<<<<<<' not in text, path
    (SRC / path).write_text(text)

paths = [
    'cc/trees/layer_tree_host.cc', 'cc/trees/layer_tree_host.h',
    'third_party/blink/public/web/web_frame_widget.h',
    'third_party/blink/renderer/platform/BUILD.gn', 'third_party/blink/renderer/core/BUILD.gn',
    'content/browser/BUILD.gn', 'content/renderer/BUILD.gn',
    'content/browser/browser_interface_binders.cc',
    'content/browser/renderer_host/render_frame_host_impl.cc',
    'content/browser/renderer_host/render_frame_host_impl.h',
    'content/renderer/render_frame_impl.cc', 'content/renderer/render_frame_impl.h',
    'components/viz/host/host_display_client.h', 'skia/BUILD.gn',
    'components/viz/service/BUILD.gn',
]
for path in paths:
    resolve(path)

# Make the layered surface available on Linux, preserving the current platform guards.
replace('components/viz/host/host_display_client.cc',
        '#if BUILDFLAG(IS_WIN)\nvoid HostDisplayClient::CreateLayeredWindowUpdater',
        'void HostDisplayClient::CreateLayeredWindowUpdater')
replace('components/viz/host/host_display_client.cc',
        '  if (!NeedsToUseLayerWindow(widget_)) {', '#if BUILDFLAG(IS_WIN)\n  if (!NeedsToUseLayerWindow(widget_)) {')
replace('components/viz/host/host_display_client.cc',
        '      std::make_unique<LayeredWindowUpdaterImpl>(widget_, std::move(receiver));\n}',
        '      std::make_unique<LayeredWindowUpdaterImpl>(widget_, std::move(receiver));\n#endif\n}\n#if BUILDFLAG(IS_WIN)')
replace('services/viz/privileged/mojom/compositing/display_private.mojom',
        '[EnableIf=is_win]\n  CreateLayeredWindowUpdater', 'CreateLayeredWindowUpdater')
replace('services/viz/privileged/mojom/compositing/layered_window_updater.mojom',
        'Draw() => ();', 'Draw(gfx.mojom.Rect damage_rect) => ();')
# Windows implementation needs the new damage parameter as well.
for path in ('components/viz/host/layered_window_updater_impl.cc',
             'components/viz/host/layered_window_updater_impl.h'):
    p = SRC / path
    p.write_text(p.read_text().replace('Draw(DrawCallback', 'Draw(const gfx::Rect& damage_rect, DrawCallback'))

# Recover the original shared-memory software device, adapting removed helpers.
for name in ('software_output_device_proxy.cc', 'software_output_device_proxy.h'):
    path = 'components/viz/service/display_embedder/' + name
    text = (REF / 'patched' / path).read_text()
    text = text.replace('#include "components/viz/common/resources/resource_sizes.h"',
                        '#include "base/numerics/checked_math.h"\n#include "base/functional/bind.h"')
    text = text.replace('#include "components/viz/service/display_embedder/output_device_backing.h"\n', '')
    text = re.sub(r'  size_t required_bytes;\n  if \(!ResourceSizes::MaybeSizeInBytes\(.*?\n  }',
        '''  size_t required_bytes;
  auto bytes = base::CheckedNumeric<size_t>(viewport_pixel_size_.width()) *
               viewport_pixel_size_.height() * 4;
  if (!bytes.AssignIfValid(&required_bytes)) return;''', text, flags=re.S)
    text = re.sub(r'  TRACE_EVENT_ASYNC_(?:BEGIN|END)0\([^\n]*\);', '', text)
    (SRC / path).write_text(text)
replace('components/viz/service/display_embedder/output_surface_provider_impl.cc',
        '#include "components/viz/service/display_embedder/output_surface_provider_impl.h"',
        '#include "components/viz/service/display_embedder/output_surface_provider_impl.h"\n#include "components/viz/service/display_embedder/software_output_device_proxy.h"')
replace('components/viz/service/display_embedder/output_surface_provider_impl.cc',
        '  if (headless_)\n    return std::make_unique<SoftwareOutputDevice>();',
        '''  if (headless_) {
    mojo::PendingRemote<mojom::LayeredWindowUpdater> updater;
    display_client->CreateLayeredWindowUpdater(updater.InitWithNewPipeAndPassReceiver());
    return std::make_unique<SoftwareOutputDeviceProxy>(std::move(updater));
  }''')
replace('content/browser/compositor/viz_process_transport_factory.cc',
        '#include "components/viz/host/host_display_client.h"',
        '#include "components/viz/host/host_display_client.h"\n#include "carbonyl/src/browser/host_display_client.h"')
replace('content/browser/compositor/viz_process_transport_factory.cc',
        'std::make_unique<HostDisplayClient>(compositor)', 'std::make_unique<carbonyl::HostDisplayClient>()')

# Adapt Skia text capture to the current device API and base64 decoder.
path = 'content/renderer/render_frame_impl.cc'
p = SRC / path
text = p.read_text()
for header in ('third_party/skia/include/svg/SkSVGCanvas.h', 'third_party/skia/include/utils/SkBase64.h'):
    text = text.replace(f'#include "{header}"\n', '')
text = text.replace('#include "third_party/skia/src/text/GlyphRun.h"',
                    '#include "third_party/skia/src/text/GlyphRun.h"\n#include "base/base64.h"')
text = text.replace('SkBaseDevice', 'SkDevice')
text = text.replace('SkDevice* onCreateDevice(const CreateInfo& info, const SkPaint*) override {\n    return new TextCaptureDevice(info.fInfo, SkSurfaceProps(0, info.fPixelGeometry));',
                    'sk_sp<SkDevice> createDevice(const CreateInfo& info, const SkPaint*) override {\n    return sk_sp<SkDevice>(new TextCaptureDevice(info.fInfo, SkSurfaceProps(0, info.fPixelGeometry)));')
text = text.replace('SkMatrix transform = device->getRelativeTransform(*this);',
                    'SkMatrix transform = device->getRelativeTransform(*this).asM33();')
text = text.replace('void drawPath(const SkPath& path, const SkPaint& paint, bool = false)',
                    'void drawPath(const SkPath& path, const SkPaint& paint)')
text = text.replace('const sktext::GlyphRunList& glyphRunList,\n                          const SkPaint&,\n                          const SkPaint& paint)',
                    'const sktext::GlyphRunList& glyphRunList,\n                          const SkPaint& paint)')
a = text.index('      size_t size = 0;', text.index('class TextCaptureDevice'))
b = text.index('      data_.push_back(', a)
text = text[:a] + '''      std::string utf8;
      if (!base::Base64Decode(std::string_view(base64, runSize), &utf8)) return;
\n''' + text[b:]
text = text.replace('std::string(utf8, size)', 'std::move(utf8)')
text = text.replace('if (auto& callback = *render_callback_.get()) {\n    callback = nullptr;\n  }',
                    'if (render_callback_) *render_callback_ = nullptr;')
p.write_text(text)

# Keep native terminal text by recording base64 text in unshaped glyph runs,
# exactly as upstream Carbonyl does, at the modern NG text entry point.
path = 'third_party/blink/renderer/platform/fonts/font.cc'
replace(path, '#include <array>', '#include <array>\n#include "base/base64.h"\n#include "carbonyl/src/browser/bridge.h"')
replace(path, '  // For performance avoid stack initialization on this large object.',
        '''  if (!carbonyl::Bridge::BitmapMode()) {
    auto string = StringView(text_info.text, text_info.from, text_info.Length()).ToString().Utf8();
    auto encoded = base::Base64Encode(string);
    auto blob = SkTextBlob::MakeFromString(encoded.c_str(),
        PrimaryFont()->PlatformData().CreateSkFont(&font_description_));
    canvas->drawTextBlob(blob, point.x(), point.y(), node_id, flags);
    return;
  }

  // For performance avoid stack initialization on this large object.''')

path = 'third_party/blink/renderer/core/css/resolver/style_resolver.cc'
replace(path, '#include "third_party/blink/renderer/core/css/resolver/style_resolver.h"',
        '#include "third_party/blink/renderer/core/css/resolver/style_resolver.h"\n#include "carbonyl/src/browser/bridge.h"')
original = (REF / 'patched' / path).read_text()
a = original.index('  if (!carbonyl::Bridge::BitmapMode())')
b = original.index('  state.LoadPendingResources();', a)
replace(path, '  state.LoadPendingResources();', original[a:b] + '  state.LoadPendingResources();')

replace(path, '    FontFamily family;\n\n    family.SetFamily("monospace", FontFamily::Type::kGenericFamily);',
        '    FontFamily family(AtomicString("monospace"), FontFamily::Type::kGenericFamily);')

# Keep sub-unit device scaling, required for the upstream 2x4 pixel cell grid.
replace('ui/display/display.cc',
        '  device_scale_factor_ = std::max(0.5f, device_scale_factor_);',
        '  device_scale_factor_ = device_scale_factor;')
replace('headless/lib/browser/headless_screen.cc',
        '#include "headless/lib/browser/headless_screen.h"',
        '#include "headless/lib/browser/headless_screen.h"\n#include "carbonyl/src/browser/bridge.h"')
replace('headless/lib/browser/headless_screen.cc',
        'HeadlessScreenInfo({.bounds = gfx::Rect(window_size)})',
        'HeadlessScreenInfo({.bounds = gfx::ScaleToEnclosedRect(gfx::Rect(window_size), carbonyl::Bridge::GetDPI()), .device_pixel_ratio = carbonyl::Bridge::GetDPI()})')
replace('headless/lib/browser/headless_platform_delegate_aura.cc',
        '#include "headless/lib/browser/headless_platform_delegate.h"',
        '#include "headless/lib/browser/headless_platform_delegate.h"\n#include "carbonyl/src/browser/bridge.h"\n#include "ui/gfx/geometry/rect_conversions.h"')
replace('headless/lib/browser/headless_platform_delegate_aura.cc',
        'SetBoundsInPixels(bounds);', 'SetBoundsInPixels(gfx::ScaleToEnclosedRect(bounds, carbonyl::Bridge::GetDPI()));')

# Persist ordinary and session cookies in the profile's native Chromium store.
replace('headless/lib/browser/headless_request_context_manager.cc',
        '    context_params->enable_encrypted_cookies = cookie_encryption_enabled_;',
        '''    context_params->persist_session_cookies = true;
    context_params->restore_old_session_cookies = true;
    context_params->enable_encrypted_cookies = cookie_encryption_enabled_;''')

# Start the upstream Rust renderer and use an observer for native navigation.
replace('headless/app/headless_shell_main.cc',
        '#include "headless/public/headless_shell.h"',
        '#include "headless/public/headless_shell.h"\n#include "carbonyl/src/browser/renderer.h"')
replace('headless/app/headless_shell_main.cc',
        'int main(int argc, const char** argv) {', 'int main(int argc, const char** argv) {\n  carbonyl::Renderer::Main();')
replace('headless/app/headless_shell.cc',
        '#include "headless/public/headless_shell.h"',
        '#include "headless/public/headless_shell.h"\n#include "carbonyl/src/browser/browser_input.h"\n#include "carbonyl/src/browser/renderer.h"')
replace('headless/app/headless_shell.cc',
        '  browser_ = browser;', '  browser_ = browser;\n  carbonyl::Renderer::GetCurrent()->StartRenderer();')
replace('headless/app/headless_shell.cc',
        '  HeadlessBrowserContext* browser_context = browser_->CreateBrowserContext();',
        '''  HeadlessBrowserContext::CreateParams context_params;
  context_params.window_size = carbonyl::Renderer::GetCurrent()->GetSize();
  context_params.block_new_web_contents = true;
  HeadlessBrowserContext* browser_context = browser_->CreateBrowserContext(std::move(context_params));''')
replace('headless/app/headless_shell.cc',
        'browser_context->CreateWebContents(target_url);', 'browser_context->CreateWebContents();')
replace('headless/app/headless_shell.cc',
        '    ShutdownSoon();\n  }\n}',
        '''    ShutdownSoon();
  } else {
    carbonyl::OpenAddress(web_contents, args.front());
    carbonyl::StartBrowserInput(browser);
  }
}''')
replace('headless/BUILD.gn',
        'source_set("headless_browser") {\n  sources = [',
        'source_set("headless_browser") {\n  sources = [\n    "//carbonyl/src/browser/browser_input.cc",\n    "//carbonyl/src/browser/browser_input.h",')
replace('headless/BUILD.gn',
        '    ":backend_cdp_bindings",\n    ":headless_common",',
        '''    ":backend_cdp_bindings",
    "//carbonyl/src/browser:renderer",
    "//carbonyl/src/browser:bridge",
    "//components/input",
    ":headless_common",''')
# Headless shell app and main both call the Carbonyl renderer directly.
replace('headless/BUILD.gn', '    "//base/version_info",',
        '    "//base/version_info",\n    "//carbonyl/src/browser:renderer",')
replace('headless/BUILD.gn', '    ":headless_shell_lib",\n    "//build:branding_buildflags",',
        '    ":headless_shell_lib",\n    "//carbonyl/src/browser:renderer",\n    "//build:branding_buildflags",')

# Skia receives the original text suppression patch at its current entry point.
path = SRC / 'third_party/skia/src/core/SkBitmapDevice.cpp'
text = path.read_text().replace('#include "src/core/SkBitmapDevice.h"',
        '#include "src/core/SkBitmapDevice.h"\n#include "carbonyl/src/browser/bridge.h"')
text = text.replace('    SkASSERT(!glyphRunList.hasRSXForm());',
                    '    if (!carbonyl::Bridge::BitmapMode()) return;\n    SkASSERT(!glyphRunList.hasRSXForm());')
path.write_text(text)

(SRC / 'carbonyl').mkdir(exist_ok=True)
for name in ('src', 'build'):
    destination = SRC / 'carbonyl' / name
    if not destination.exists(): destination.symlink_to(ROOT / name, target_is_directory=True)
print('Port applied. Save and review git diffs before building.')
