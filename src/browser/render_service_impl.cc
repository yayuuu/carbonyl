#include "carbonyl/src/browser/render_service_impl.h"

#include <iostream>
#include <map>
#include "base/no_destructor.h"

#include "carbonyl/src/browser/renderer.h"

namespace carbonyl {
namespace {
struct LastText { base::RepeatingCallback<Renderer*()> renderer; std::vector<Text> text; };
auto& TextFrames() {
  static base::NoDestructor<std::map<CarbonylRenderServiceImpl*,LastText>> frames;
  return *frames;
}
}
// A newly-created window may finish its first paint before its terminal
// frontend attaches. Replay that native text instead of losing it forever.
CARBONYL_RENDERER_EXPORT void ReplayTerminalText(Renderer* renderer) {
  for (auto& [service,frame] : TextFrames())
    if (frame.renderer.Run()==renderer && !frame.text.empty()) renderer->DrawText(frame.text);
}


CarbonylRenderServiceImpl::CarbonylRenderServiceImpl(
    mojo::PendingReceiver<mojom::CarbonylRenderService> receiver, base::RepeatingCallback<Renderer*()> renderer):
    renderer_(std::move(renderer)),
    receiver_(this, std::move(receiver))
{ TextFrames()[this].renderer=renderer_; }

CarbonylRenderServiceImpl::~CarbonylRenderServiceImpl() { TextFrames().erase(this); }

void CarbonylRenderServiceImpl::DrawText(std::vector<mojom::TextDataPtr> data) {
    std::vector<Text> mapped;

    for (auto& text: data) {
        mapped.emplace_back(text->contents, text->bounds, text->color);
    }

    TextFrames()[this].text=mapped;
    if (auto* renderer=renderer_.Run()) renderer->DrawText(mapped);
}

}
