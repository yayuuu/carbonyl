#include "carbonyl/src/browser/renderer.h"

#include <memory>
#include <map>
#include <mutex>
#include <iostream>
#include <stdio.h>

#include "base/functional/callback.h"
#include "carbonyl/src/browser/bridge.h"
#include "ui/gfx/geometry/rect_f.h"
#include "third_party/skia/include/core/SkColor.h"

extern "C" {

struct carbonyl_renderer_size {
    unsigned int width;
    unsigned int height;
};
struct carbonyl_renderer_point {
    unsigned int x;
    unsigned int y;
};
struct carbonyl_renderer_rect {
    struct carbonyl_renderer_point origin;
    struct carbonyl_renderer_size size;
};
struct carbonyl_renderer_color {
    uint8_t r;
    uint8_t g;
    uint8_t b;
};
struct carbonyl_renderer_text {
    const char* text;
    carbonyl_renderer_rect rect;
    carbonyl_renderer_color color;
};

void carbonyl_bridge_main();
bool carbonyl_bridge_bitmap_mode();
float carbonyl_bridge_get_dpi();

struct carbonyl_renderer* carbonyl_renderer_create();
struct carbonyl_renderer* carbonyl_renderer_create_terminal(int fd);
void carbonyl_renderer_stop(struct carbonyl_renderer*);
bool carbonyl_renderer_changed(struct carbonyl_renderer*);
void carbonyl_renderer_start(struct carbonyl_renderer* renderer);
void carbonyl_renderer_resize(struct carbonyl_renderer* renderer);
struct carbonyl_renderer_size carbonyl_renderer_get_size(struct carbonyl_renderer* renderer);
void carbonyl_renderer_push_nav(struct carbonyl_renderer* renderer, const char* url, bool can_go_back, bool can_go_forward);
void carbonyl_renderer_set_title(struct carbonyl_renderer* renderer, const char* title);
void carbonyl_renderer_update_scroll(struct carbonyl_renderer*, float, float, float);
void carbonyl_renderer_show_dialog(struct carbonyl_renderer*, uint64_t, const char*,
                                    const carbonyl_dialog_control*, size_t);
void carbonyl_renderer_clipboard_write(struct carbonyl_renderer*, const char*);
void carbonyl_renderer_clipboard_query(struct carbonyl_renderer*);
void carbonyl_renderer_download_status(struct carbonyl_renderer*, const char*);
void carbonyl_renderer_clear_text(struct carbonyl_renderer* renderer);
void carbonyl_renderer_listen(struct carbonyl_renderer* renderer, const struct carbonyl_renderer_browser_delegate* delegate);
void carbonyl_renderer_draw_text(
    struct carbonyl_renderer* renderer,
    const struct carbonyl_renderer_text* text,
    size_t text_size
);
void carbonyl_renderer_draw_bitmap(
    struct carbonyl_renderer* renderer,
    const unsigned char* pixels,
    const struct carbonyl_renderer_size size,
    const struct carbonyl_renderer_rect rect,
    void (*callback) (void*),
    void* callback_data
);

}

namespace carbonyl {

namespace {
    static std::unique_ptr<Renderer> globalInstance;
    thread_local Renderer* scoped = nullptr;
    Renderer* last_active = nullptr;
    std::map<gfx::AcceleratedWidget, Renderer*> terminals;
    std::mutex terminals_mutex;
}

Renderer::Renderer(struct carbonyl_renderer* ptr): ptr_(ptr) {}

void Renderer::Main() {
    carbonyl_bridge_main();

    Bridge::Configure(
        carbonyl_bridge_get_dpi(),
        carbonyl_bridge_bitmap_mode()
    );
}

Renderer* Renderer::GetCurrent() {
    if (scoped) return scoped;
    if (last_active) return last_active;
    if (!globalInstance) {
        globalInstance = std::unique_ptr<Renderer>(
            new Renderer(carbonyl_renderer_create())
        );
    }

    return globalInstance.get();
}

Renderer* Renderer::CreateTerminal(int fd) {
    // Kept for process lifetime: Viz and posted input callbacks may still refer
    // to a terminal after its browser window closes.
    return new Renderer(carbonyl_renderer_create_terminal(fd));
}
Renderer::Scope::Scope(Renderer* renderer, bool active) : previous_(scoped) {
    scoped = renderer; if (active) last_active = renderer;
}
Renderer::Scope::~Scope() { scoped = previous_; }
Renderer* Renderer::ForWidget(gfx::AcceleratedWidget widget) {
    std::lock_guard<std::mutex> lock(terminals_mutex);
    auto it = terminals.find(widget);
    return it == terminals.end() ? nullptr : it->second;
}
void Renderer::BindWidget(gfx::AcceleratedWidget widget) {
    std::lock_guard<std::mutex> lock(terminals_mutex); terminals[widget] = this;
}
void Renderer::Stop() {
    started_ = false; carbonyl_renderer_stop(ptr_);
    std::lock_guard<std::mutex> lock(terminals_mutex);
    std::erase_if(terminals, [this](const auto& entry) {return entry.second == this;});
    if (last_active == this) last_active = terminals.empty() ? nullptr : terminals.begin()->second;
}
bool Renderer::SizeChanged() { return carbonyl_renderer_changed(ptr_); }
void Renderer::StartRenderer() {
    started_ = true;
    carbonyl_renderer_start(ptr_);
}

gfx::Size Renderer::GetSize() {
    auto size = carbonyl_renderer_get_size(ptr_);

    return gfx::Size(size.width, size.height);
}

gfx::Size Renderer::Resize() {
    carbonyl_renderer_resize(ptr_);
    Bridge::Resize();

    return GetSize();
}

void Renderer::Listen(const struct carbonyl_renderer_browser_delegate* delegate) {
    carbonyl_renderer_listen(ptr_, delegate);
}

void Renderer::PushNav(const std::string& url, bool can_go_back, bool can_go_forward) {
    if (!url.size()) {
        return;
    }

    carbonyl_renderer_push_nav(ptr_, url.c_str(), can_go_back, can_go_forward);
}

void Renderer::SetTitle(const std::string& title) {
    if (!title.size()) {
        return;
    }

    carbonyl_renderer_set_title(ptr_, title.c_str());
}

void Renderer::UpdateScroll(float offset, float viewport, float content) {
    carbonyl_renderer_update_scroll(ptr_, offset, viewport, content);
}

void Renderer::SetDownloadStatus(const std::string& text) { carbonyl_renderer_download_status(ptr_, text.c_str()); }

void Renderer::WriteClipboard(const std::string& encoded) {
    carbonyl_renderer_clipboard_write(ptr_, encoded.c_str());
}
void Renderer::QueryClipboard() { carbonyl_renderer_clipboard_query(ptr_); }
void Renderer::ShowDialog(uint64_t id, const std::string& title,
                          const std::vector<carbonyl_dialog_control>& controls) {
    carbonyl_renderer_show_dialog(ptr_, id, title.c_str(), controls.data(), controls.size());
}

void Renderer::DrawText(const std::vector<Text>& text) {
    struct carbonyl_renderer_text data[text.size()];

    for (size_t i = 0; i < text.size(); i++) {
        data[i].text = text[i].text.c_str();
        data[i].color.r = SkColorGetR(text[i].color);
        data[i].color.g = SkColorGetG(text[i].color);
        data[i].color.b = SkColorGetB(text[i].color);
        data[i].rect.origin.x = text[i].rect.x();
        data[i].rect.origin.y = text[i].rect.y();
        data[i].rect.size.width = std::ceil(text[i].rect.width());
        data[i].rect.size.height = std::ceil(text[i].rect.height());
    }

    carbonyl_renderer_draw_text(ptr_, data, text.size());
}

void Renderer::DrawBitmap(
    const unsigned char* pixels,
    const gfx::Size& pixels_size,
    const gfx::Rect& damage,
    base::OnceCallback<void()> callback
) {
    if (!started_) { std::move(callback).Run(); return; }
    auto* box = new base::OnceCallback<void()>(std::move(callback));

    carbonyl_renderer_draw_bitmap(
        ptr_,
        pixels,
        {
            .width = (unsigned int)pixels_size.width(),
            .height = (unsigned int)pixels_size.height(),
        },
        {
            .origin = {
                .x = (unsigned int)damage.x(),
                .y = (unsigned int)damage.y(),
            },
            .size = {
                .width = (unsigned int)damage.width(),
                .height = (unsigned int)damage.height(),
            },
        },
        [](void* box) {
            auto* ptr = static_cast<base::OnceCallback<void()>*>(box);

            std::move(*ptr).Run();
            delete ptr;
        },
        box
    );
}

}
