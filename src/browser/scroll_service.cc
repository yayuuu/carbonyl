#include "carbonyl/src/browser/scroll_service.h"
#include <cmath>
#include <map>
#include "carbonyl/src/browser/browser_input.h"
#include "carbonyl/src/browser/scroll.mojom.h"
#include "carbonyl/src/browser/renderer.h"
#include "content/public/browser/render_frame_host.h"
#include "content/public/browser/web_contents.h"
#include "mojo/public/cpp/bindings/remote.h"
#include "mojo/public/cpp/bindings/self_owned_receiver.h"

namespace carbonyl {
namespace {
class ScrollService;
std::map<Renderer*, ScrollService*> active;
class ScrollService final : public mojom::ScrollService {
 public:
  explicit ScrollService(content::RenderFrameHost* frame) : frame_id_(frame->GetGlobalId()) {}
  ~ScrollService() override { std::erase_if(active,[this](const auto& p){return p.second==this;}); }
  void BindController(mojo::PendingRemote<mojom::ScrollController> controller) override {
    controller_.reset(); controller_.Bind(std::move(controller));
  }
  bool IsVisible() {
    auto* frame = content::RenderFrameHost::FromID(frame_id_);
    auto* contents = frame ? content::WebContents::FromRenderFrameHost(frame) : nullptr;
    return contents && contents->GetPrimaryMainFrame() == frame &&
        contents->GetVisibility() == content::Visibility::VISIBLE;
  }
  void Update(float offset, float viewport, float content) override {
    if (!IsVisible()) return;
    auto* frame=content::RenderFrameHost::FromID(frame_id_);
    auto* renderer=RendererForContents(frame ? content::WebContents::FromRenderFrameHost(frame) : nullptr);
    if (renderer) { active[renderer]=this; renderer->UpdateScroll(offset, viewport, content); }
  }
  void SetOffset(float offset) {
    if (IsVisible() && controller_.is_bound() && std::isfinite(offset))
      controller_->SetOffset(offset);
  }
 private:
  content::GlobalRenderFrameHostId frame_id_;
  mojo::Remote<mojom::ScrollController> controller_;
};
}
void BindScrollService(content::RenderFrameHost* frame, mojo::PendingReceiver<mojom::ScrollService> receiver) {
  mojo::MakeSelfOwnedReceiver(std::make_unique<ScrollService>(frame), std::move(receiver));
}
void ScrollPageTo(float offset) { auto it=active.find(Renderer::GetCurrent()); if (it!=active.end()) it->second->SetOffset(offset); }
}
