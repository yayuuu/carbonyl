#ifndef CARBONYL_SRC_BROWSER_SCROLL_SERVICE_H_
#define CARBONYL_SRC_BROWSER_SCROLL_SERVICE_H_
#include "content/common/content_export.h"
#include "carbonyl/src/browser/scroll.mojom-forward.h"
#include "mojo/public/cpp/bindings/pending_receiver.h"
namespace content { class RenderFrameHost; }
namespace carbonyl {
CONTENT_EXPORT void BindScrollService(content::RenderFrameHost*, mojo::PendingReceiver<mojom::ScrollService>);
CONTENT_EXPORT void ScrollPageTo(float offset);
}
#endif
