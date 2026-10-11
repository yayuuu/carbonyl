#ifndef CARBONYL_SRC_BROWSER_RENDERER_H_
#define CARBONYL_SRC_BROWSER_RENDERER_H_

#include <cstdint>
#include <functional>
#include <vector>
#include "ui/gfx/native_ui_types.h"

#include "base/functional/callback.h"
#include "base/memory/raw_ptr_exclusion.h"
#include "base/memory/raw_ptr.h"
#include "carbonyl/src/browser/export.h"
#include "ui/gfx/geometry/rect_f.h"

extern "C" {

struct carbonyl_dialog_control {
    int id;
    int kind; // 0 label, 1 button, 2 checkbox, 3 editable text
    const char* label;
    const char* value;
    unsigned int flags; // enabled, checked, password, initially focused
};
struct carbonyl_renderer;
struct carbonyl_renderer_browser_delegate {
    void (*shutdown) ();
    void (*refresh) ();
    void (*go_to) (const char* url);
    void (*go_back) ();
    void (*go_forward) ();
    void (*scroll) (int);
    void (*key_press) (char);
    void (*mouse_up) (unsigned int, unsigned int);
    void (*mouse_down) (unsigned int, unsigned int);
    void (*mouse_move) (unsigned int, unsigned int);
    void (*post_task) (uintptr_t, void (*)(void*), void*);
    void (*scroll_to) (float);
    void (*dialog_action) (uint64_t, int, int, const char*);
    void (*clipboard_text) (const char*, bool);
    uintptr_t context;
};

} /* end extern "C" */

namespace carbonyl {

struct CARBONYL_RENDERER_EXPORT Text {
    Text(
        std::string text,
        gfx::RectF rect,
        uint32_t color
    ):
        text(text),
        rect(rect),
        color(color)
    {}

    std::string text;
    gfx::RectF rect;
    uint32_t color;
};

class CARBONYL_RENDERER_EXPORT Renderer {
public:
    static void Main();
    static Renderer* GetCurrent();
    static Renderer* CreateTerminal(int fd);
    static Renderer* ForWidget(gfx::AcceleratedWidget widget);
    void BindWidget(gfx::AcceleratedWidget widget);
    void Stop();
    bool SizeChanged();
    class CARBONYL_RENDERER_EXPORT Scope {
     public: explicit Scope(Renderer* renderer, bool active = false); ~Scope();
     private: raw_ptr<Renderer> previous_;
    };

    gfx::Size GetSize();

    gfx::Size Resize();
    void StartRenderer();
    void Listen(const struct carbonyl_renderer_browser_delegate* delegate);
    void PushNav(const std::string& url, bool can_go_back, bool can_go_forward);
    void SetTitle(const std::string& title);
    void UpdateScroll(float offset, float viewport, float content);
    void SetDownloadStatus(const std::string& text);
    void WriteClipboard(const std::string& encoded);
    void QueryClipboard();
    void ShowDialog(uint64_t id, const std::string& title,
                    const std::vector<carbonyl_dialog_control>& controls);
    void DrawText(const std::vector<Text>& text);
    void DrawBitmap(
        const unsigned char* pixels,
        const gfx::Size& size,
        const gfx::Rect& damage,
        base::OnceCallback<void()> callback
    );

private:
    Renderer(struct carbonyl_renderer* ptr);

    // Foreign allocation owned by the Rust renderer, outside PartitionAlloc.
    RAW_PTR_EXCLUSION struct carbonyl_renderer* ptr_;
    bool started_ = false;
};

}

#endif  // CARBONYL_SRC_BROWSER_RENDERER_H_
