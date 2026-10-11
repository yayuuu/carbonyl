// Dispatch terminal input through Aura, including popup capture and Chromium's
// normal hit testing for isolated iframes and drag selection.
#include "carbonyl/src/browser/frame_input.h"
#include "content/public/browser/render_widget_host_view.h"
#include "carbonyl/src/browser/browser_input.h"
#include "carbonyl/src/browser/renderer.h"
#include "base/functional/bind.h"
#include "base/no_destructor.h"
#include "mojo/public/cpp/bindings/remote.h"
#include <map>
#include "content/public/browser/web_contents.h"
#include "third_party/blink/public/common/input/web_mouse_event.h"
#include "third_party/blink/public/common/input/web_mouse_wheel_event.h"
#include "ui/aura/window.h"
#include "ui/aura/window_tree_host.h"
#include "ui/aura/window_event_dispatcher.h"
#include "ui/events/event.h"
#include "ui/gfx/geometry/transform.h"
namespace carbonyl {
namespace {
aura::WindowTreeHost* Host(content::WebContents* contents,gfx::PointF& point) {
  auto* view=contents->GetRenderWidgetHostView();
  auto* native=view ? view->GetNativeView() : nullptr;
  if(!native)return nullptr;
  auto* root=native->GetRootWindow();
  aura::Window::ConvertPointToTarget(native,root,&point);
  return root->GetHost();
}
void Dispatch(aura::WindowTreeHost* host,ui::LocatedEvent& event) {
  event.UpdateForRootTransform(host->GetRootTransform(),host->GetRootTransformForLocalEventCoordinates());
  host->dispatcher()->OnEventFromSource(&event);
}
}
void RouteFrameMouse(content::WebContents* contents,const blink::WebMouseEvent& source) {
  auto point=source.PositionInWidget();auto* host=Host(contents,point);if(!host)return;
  bool down=(source.GetModifiers() & blink::WebInputEvent::kLeftButtonDown)!=0;
  auto type=source.GetType()==blink::WebInputEvent::Type::kMouseDown ? ui::EventType::kMousePressed :
            source.GetType()==blink::WebInputEvent::Type::kMouseUp ? ui::EventType::kMouseReleased :
            down ? ui::EventType::kMouseDragged : ui::EventType::kMouseMoved;
  int flags=down ? ui::EF_LEFT_MOUSE_BUTTON : ui::EF_NONE;
  int changed=type==ui::EventType::kMousePressed || type==ui::EventType::kMouseReleased ? ui::EF_LEFT_MOUSE_BUTTON : 0;
  ui::MouseEvent event(type,point,point,source.TimeStamp(),flags,changed);
  event.SetClickCount(source.click_count);Dispatch(host,event);
}
void RouteFrameWheel(content::WebContents* contents,blink::WebMouseWheelEvent& source) {
  if(source.delta_x==0 && source.delta_y==0)return; // Aura handles wheel end.
  auto point=source.PositionInWidget();auto* host=Host(contents,point);if(!host)return;
  ui::MouseWheelEvent event(gfx::Vector2d(source.delta_x,source.delta_y),point,point,source.TimeStamp(),ui::EF_NONE,0);
  Dispatch(host,event);
}

namespace {
struct FramePopup {
  uint64_t id;
  mojo::Remote<blink::mojom::PopupMenuClient> client;
  std::vector<blink::mojom::MenuItemPtr> items;
};
auto& Popups() {
  static base::NoDestructor<std::map<Renderer*, std::unique_ptr<FramePopup>>> popups;
  return *popups;
}
void FinishPopup(Renderer* renderer, uint64_t id, int option) {
  auto it=Popups().find(renderer);
  if(it==Popups().end() || it->second->id!=id)return;
  auto popup=std::move(it->second);Popups().erase(it);
  renderer->ShowDialog(0,"",{});
  if(option>=0)popup->client->DidAcceptIndices({option});
  else popup->client->DidCancel();
}
}
void CloseFramePopup(Renderer* renderer) {
  auto it=Popups().find(renderer);
  if(it!=Popups().end())FinishPopup(renderer,it->second->id,-1);
}
void ShowFramePopup(content::RenderFrameHost* frame,
                    mojo::PendingRemote<blink::mojom::PopupMenuClient> client,
                    std::vector<blink::mojom::MenuItemPtr> items, int selected) {
  auto* renderer=RendererForContents(content::WebContents::FromRenderFrameHost(frame));
  if(!renderer) {mojo::Remote<blink::mojom::PopupMenuClient> remote(std::move(client));remote->DidCancel();return;}
  CloseFramePopup(renderer);
  static uint64_t serial=uint64_t{1}<<63;
  auto popup=std::make_unique<FramePopup>();popup->id=++serial;
  popup->client.Bind(std::move(client));popup->items=std::move(items);
  popup->client.set_disconnect_handler(base::BindOnce(&FinishPopup,renderer,popup->id,-1));
  std::vector<carbonyl_dialog_control> controls;
  for(size_t i=0;i<popup->items.size();++i) {
    auto& item=popup->items[i];
    bool option=item->type==blink::mojom::MenuItem::Type::kOption || item->type==blink::mojom::MenuItem::Type::kCheckableOption;
    unsigned flags=(option && item->enabled ? 1u : 0u) | (static_cast<int>(i)==selected ? 8u : 0u);
    controls.push_back({static_cast<int>(i+1),option ? 1 : 0,item->label ? item->label->c_str() : "", "",flags});
  }
  renderer->ShowDialog(popup->id,"Wybierz opcję",controls);
  Popups()[renderer]=std::move(popup);
}
bool FramePopupAction(uint64_t id,int control,int action) {
  if(!(id & (uint64_t{1}<<63)))return false;
  auto* renderer=Renderer::GetCurrent();auto it=Popups().find(renderer);
  if(it==Popups().end() || it->second->id!=id)return true;
  if(action==2 || control==-1) {FinishPopup(renderer,id,-1);return true;}
  int index=control-1;
  if(action==0 && index>=0 && static_cast<size_t>(index)<it->second->items.size()) {
    auto& item=it->second->items[index];
    if(item->enabled && (item->type==blink::mojom::MenuItem::Type::kOption || item->type==blink::mojom::MenuItem::Type::kCheckableOption))FinishPopup(renderer,id,index);
  }
  return true;
}
}  // namespace carbonyl
