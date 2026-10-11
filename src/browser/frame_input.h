#ifndef CARBONYL_SRC_BROWSER_FRAME_INPUT_H_
#define CARBONYL_SRC_BROWSER_FRAME_INPUT_H_
#include "content/common/content_export.h"
#include "third_party/blink/public/mojom/choosers/popup_menu.mojom.h"
namespace content { class WebContents; class RenderFrameHost; }
namespace blink { class WebMouseEvent; class WebMouseWheelEvent; }
namespace carbonyl {
class Renderer;
CONTENT_EXPORT void ShowFramePopup(content::RenderFrameHost*, mojo::PendingRemote<blink::mojom::PopupMenuClient>, std::vector<blink::mojom::MenuItemPtr>, int selected);
CONTENT_EXPORT bool FramePopupAction(uint64_t id, int control, int action);
CONTENT_EXPORT void CloseFramePopup(Renderer*);
CONTENT_EXPORT void RouteFrameMouse(content::WebContents*,const blink::WebMouseEvent&);
CONTENT_EXPORT void RouteFrameWheel(content::WebContents*,blink::WebMouseWheelEvent&);
}
#endif
