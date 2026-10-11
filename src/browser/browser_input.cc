#include "carbonyl/src/browser/browser_input.h"
#include "carbonyl/src/browser/frame_input.h"
#include "carbonyl/src/browser/scroll_service.h"

#include <csignal>
#include <memory>
#include <map>
#include "ui/aura/window.h"
#include "ui/aura/window_tree_host.h"
#include "base/functional/bind.h"
#include "base/no_destructor.h"
#include "base/strings/string_util.h"
#include "base/strings/utf_string_conversions.h"
#include "base/task/single_thread_task_runner.h"
#include "base/timer/timer.h"
#include "carbonyl/src/browser/renderer.h"
#include "components/input/native_web_keyboard_event.h"
#include "content/public/browser/browser_task_traits.h"
#include "content/public/browser/browser_thread.h"
#include "content/public/browser/navigation_controller.h"
#include "content/public/browser/navigation_entry.h"
#include "content/public/browser/navigation_handle.h"
#include "content/public/browser/render_frame_host.h"
#include "content/public/browser/render_view_host.h"
#include "content/public/browser/render_widget_host.h"
#include "content/public/browser/render_widget_host_view.h"
#include "content/browser/renderer_host/render_widget_host_impl.h"
#include "third_party/blink/public/mojom/input/input_handler.mojom.h"
#include "content/public/browser/web_contents.h"
#include "content/public/browser/web_contents_observer.h"
#include "content/public/browser/web_contents_user_data.h"
#include "net/base/net_errors.h"
#include "third_party/blink/public/common/input/web_mouse_event.h"
#include "third_party/blink/public/common/input/web_mouse_wheel_event.h"
#include "ui/events/keycodes/keyboard_codes.h"
#include "ui/events/keycodes/dom/dom_key.h"
#include "ui/base/page_transition_types.h"

namespace carbonyl {
namespace {
std::map<Renderer*, std::unique_ptr<BrowserInputClient>> clients;
BrowserInputClient* CurrentClient() {
  auto it=clients.find(Renderer::GetCurrent()); return it==clients.end() ? nullptr : it->second.get();
}
struct MouseState {unsigned int x=0,y=0;bool left=false;};
std::map<Renderer*,MouseState> mouse_states;
volatile std::sig_atomic_t resize_pending = 0;

class AddressObserver : public content::WebContentsObserver,
                        public content::WebContentsUserData<AddressObserver> {
 public:
  void Open(const std::string& address) {
    std::string input(base::TrimWhitespaceASCII(address, base::TRIM_ALL));
    GURL explicit_url(input);
    // A host followed by a numeric port is not a URL scheme.
    bool explicit_scheme = (explicit_url.has_scheme() && input.find("://") == explicit_url.scheme().size()) ||
        explicit_url.SchemeIs("http") || explicit_url.SchemeIs("https") ||
        explicit_url.SchemeIs("about") || explicit_url.SchemeIs("data") ||
        explicit_url.SchemeIs("file") || explicit_url.SchemeIs("chrome") ||
        explicit_url.SchemeIs("blob") || explicit_url.SchemeIs("javascript");
    GURL url = explicit_scheme ? explicit_url : GURL("https://" + input);
    if (!url.is_valid()) return;
    fallback_url_ = explicit_scheme ? GURL() : url;
    content::NavigationController::LoadURLParams params(url);
    params.transition_type = ui::PageTransitionFromInt(
        ui::PAGE_TRANSITION_TYPED | ui::PAGE_TRANSITION_FROM_ADDRESS_BAR);
    web_contents()->GetController().LoadURLWithParams(params);
    web_contents()->Focus();
  }

 private:
  friend class content::WebContentsUserData<AddressObserver>;
  explicit AddressObserver(content::WebContents* contents) : content::WebContentsObserver(contents),
      content::WebContentsUserData<AddressObserver>(*contents) {}

  void TitleWasSet(content::NavigationEntry* entry) override {
    if (auto* renderer=RendererForContents(web_contents())) renderer->SetTitle(base::UTF16ToUTF8(entry->GetTitleForDisplay()));
  }
  void DidStartNavigation(content::NavigationHandle* handle) override {
    if (handle->IsInPrimaryMainFrame() && !fallback_url_.is_empty() &&
        handle->GetURL() != fallback_url_) {
      fallback_url_ = GURL();
    }
  }
  void DidFinishNavigation(content::NavigationHandle* handle) override {
    if (!handle->IsInPrimaryMainFrame()) return;
    // Only retry an automatically chosen HTTPS URL, for transport failures.
    // Certificate failures and explicitly entered HTTPS addresses stay HTTPS.
    bool transport_failure = false;
    switch (handle->GetNetErrorCode()) {
      case net::ERR_CONNECTION_REFUSED:
      case net::ERR_CONNECTION_RESET:
      case net::ERR_CONNECTION_CLOSED:
      case net::ERR_CONNECTION_TIMED_OUT:
      case net::ERR_TIMED_OUT:
      case net::ERR_SSL_PROTOCOL_ERROR:
      case net::ERR_SSL_VERSION_OR_CIPHER_MISMATCH:
      case net::ERR_EMPTY_RESPONSE:
        transport_failure = true;
        break;
      default:
        break;
    }
    if (transport_failure && !fallback_url_.is_empty() &&
        handle->GetURL() == fallback_url_ && fallback_url_.SchemeIs("https")) {
      GURL::Replacements replacements;
      replacements.SetSchemeStr("http");
      GURL fallback = fallback_url_.ReplaceComponents(replacements);
      fallback_url_ = GURL();
      web_contents()->GetController().LoadURL(fallback, content::Referrer(),
                                             ui::PAGE_TRANSITION_TYPED, std::string());
      return;
    }
    fallback_url_ = GURL();
    auto& navigation = web_contents()->GetController();
    if (auto* renderer=RendererForContents(web_contents())) renderer->PushNav(handle->GetURL().spec(), navigation.CanGoBack(), navigation.CanGoForward());
  }
  GURL fallback_url_;
  WEB_CONTENTS_USER_DATA_KEY_DECL();
};
WEB_CONTENTS_USER_DATA_KEY_IMPL(AddressObserver);

void OnRefreshInput() {
  auto* browser = CurrentClient();
  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto& nav = contents->GetController();

    nav.Reload(content::ReloadType::NORMAL, true);
  }
}

void OnGoToInput(const char* address) {
  auto* browser = CurrentClient();
  if (!browser) return;
  auto pages = browser->pages.Run();
  if (!pages.empty()) OpenAddress(pages.front(), address);
}

void OnGoBackInput() {
  auto* browser = CurrentClient();
  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto& nav = contents->GetController();

    if (nav.CanGoBack()) {
      nav.GoBack();
    }
  }
}
void OnGoForwardInput() {
  auto* browser = CurrentClient();
  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto& nav = contents->GetController();

    if (nav.CanGoForward()) {
      nav.GoForward();
    }
  }
}

void OnScrollInput(int delta) {
  auto* browser = CurrentClient();
  blink::WebMouseWheelEvent event;

  event.SetType(blink::WebInputEvent::Type::kMouseWheel);
  event.SetTimeStamp(base::TimeTicks::Now());
  const auto& mouse=mouse_states[Renderer::GetCurrent()];
  event.SetPositionInWidget(mouse.x,mouse.y);
  event.SetPositionInScreen(mouse.x,mouse.y);

  event.delta_y = delta;
  event.phase = blink::WebMouseWheelEvent::kPhaseBegan;
  event.dispatch_type = blink::WebInputEvent::DispatchType::kBlocking;

  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto *host = contents->GetRenderViewHost()->GetWidget();

    if (!host) {
      continue;
    }

    RouteFrameWheel(contents,event);
  }

  // Send a synthetic wheel event with phaseEnded to finish scrolling.
  event.delta_y = 0;
  event.phase = blink::WebMouseWheelEvent::kPhaseEnded;
  event.dispatch_type = blink::WebInputEvent::DispatchType::kEventNonBlocking;
  event.has_synthetic_phase = true;

  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto *host = contents->GetRenderViewHost()->GetWidget();

    if (!host) {
      continue;
    }

    RouteFrameWheel(contents,event);
  }
}

void OnKeyPressInput(char key) {
  auto* browser = CurrentClient();
  if (browser && (key==1 || key==0x18 || key==0x16)) {
    if (browser->focus) browser->focus.Run();
    for(auto* contents:browser->pages.Run()) {
      if(key==1)contents->SelectAll();
      else if(key==0x18)contents->Cut();
      else contents->Paste();
    }
    return;
  }
  if (browser && browser->focus) browser->focus.Run();
  bool raw = true;
  input::NativeWebKeyboardEvent event(
      blink::WebKeyboardEvent::Type::kRawKeyDown,
      blink::WebInputEvent::kNoModifiers,
      base::TimeTicks::Now());

  // TODO(fathy): support IME
  switch (key) {
    case 0x11:
      event.windows_key_code = ui::KeyboardCode::VKEY_UP;
      event.dom_key = ui::DomKey::ARROW_UP;
      break;
    case 0x12:
      event.windows_key_code = ui::KeyboardCode::VKEY_DOWN;
      event.dom_key = ui::DomKey::ARROW_DOWN;
      break;
    case 0x13:
      event.windows_key_code = ui::KeyboardCode::VKEY_RIGHT;
      event.dom_key = ui::DomKey::ARROW_RIGHT;
      break;
    case 0x14:
      event.windows_key_code = ui::KeyboardCode::VKEY_LEFT;
      event.dom_key = ui::DomKey::ARROW_LEFT;
      break;
    case 0x7f:
      event.windows_key_code = ui::KeyboardCode::VKEY_BACK;
      event.dom_key = ui::DomKey::BACKSPACE;
      break;
    case 0x0d:
      raw = false;
      event.windows_key_code = ui::VKEY_RETURN;
      event.dom_key = ui::DomKey::ENTER;
      event.text[0] = '\r';
      event.unmodified_text[0] = '\r';
      break;
    case 0x09:
      event.windows_key_code = ui::VKEY_TAB;
      event.dom_key = ui::DomKey::TAB;
      break;
    default:
      raw = false;
      auto character = static_cast<unsigned char>(key);
      event.dom_key = ui::DomKey::FromCharacter(character);
      event.text[0] = character;
      event.unmodified_text[0] = character;
      if (base::IsAsciiAlpha(character) || base::IsAsciiDigit(character))
        event.windows_key_code = base::ToUpperASCII(character);
  }

  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto* frame=contents->GetFocusedFrame();
    auto* host=frame ? frame->GetRenderWidgetHost() : contents->GetRenderViewHost()->GetWidget();

    if (!host) {
      continue;
    }

    event.SetType(
      raw
        ? blink::WebKeyboardEvent::Type::kRawKeyDown
        : blink::WebKeyboardEvent::Type::kKeyDown
    );
    host->ForwardKeyboardEvent(event);

    event.SetType(blink::WebKeyboardEvent::Type::kKeyUp);
    host->ForwardKeyboardEvent(event);
  }
}

void OnMouseUpInput(unsigned int x, unsigned int y) {
  mouse_states[Renderer::GetCurrent()]={x,y,false};
  auto* browser = CurrentClient();
  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto *host = contents->GetRenderViewHost()->GetWidget();

    if (!host) {
      continue;
    }

    blink::WebMouseEvent event;

    event.button = blink::WebMouseEvent::Button::kLeft;
    event.click_count = 1;
    event.SetType(blink::WebInputEvent::Type::kMouseUp);
    event.SetTimeStamp(base::TimeTicks::Now());
    event.SetPositionInWidget(x, y);
    event.SetPositionInScreen(x, y);

    RouteFrameMouse(contents,event);
  }
}

void OnMouseDownInput(unsigned int x, unsigned int y) {
  mouse_states[Renderer::GetCurrent()]={x,y,true};
  auto* browser = CurrentClient();
  if (browser && browser->focus) browser->focus.Run();
  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto *host = contents->GetRenderViewHost()->GetWidget();

    if (!host) {
      continue;
    }

    blink::WebMouseEvent event;

    event.SetModifiers(blink::WebInputEvent::kLeftButtonDown);
    event.button = blink::WebMouseEvent::Button::kLeft;
    event.click_count = 1;
    event.SetType(blink::WebInputEvent::Type::kMouseDown);
    event.SetTimeStamp(base::TimeTicks::Now());
    event.SetPositionInWidget(x, y);
    event.SetPositionInScreen(x, y);

    RouteFrameMouse(contents,event);
  }
}

void OnMouseMoveInput(unsigned int x, unsigned int y) {
  auto* browser = CurrentClient();
  if (!browser) return;
  for (auto* contents : browser->pages.Run()) {
    auto *host = contents->GetRenderViewHost()->GetWidget();

    if (!host) {
      continue;
    }

    blink::WebMouseEvent event;

    auto& mouse=mouse_states[Renderer::GetCurrent()];mouse.x=x;mouse.y=y;
    if(mouse.left) {event.SetModifiers(blink::WebInputEvent::kLeftButtonDown);event.button=blink::WebMouseEvent::Button::kLeft;}

    event.click_count = 1;
    event.SetType(blink::WebInputEvent::Type::kMouseMove);
    event.SetTimeStamp(base::TimeTicks::Now());
    event.SetPositionInWidget(x, y);
    event.SetPositionInScreen(x, y);

    RouteFrameMouse(contents,event);
  }
}

void OnShutdownInput() {
  auto* browser = CurrentClient();
  if (browser) {
    for(auto* contents:browser->pages.Run()) {
      auto* view=contents->GetRenderWidgetHostView();
      if(view && !view->GetSelectedText().empty()) {
        if(browser->focus)browser->focus.Run();contents->Copy();return;
      }
    }
    browser->shutdown.Run();
  }
}

void CheckResize() {
  bool forced = resize_pending; resize_pending = 0;
  for (const auto& [renderer, browser] : clients) {
    if (forced || renderer->SizeChanged()) {
      Renderer::Scope scope(renderer); browser->resize.Run(renderer->Resize());
    }
  }
}
}  // namespace

Renderer* RendererForContents(content::WebContents* contents) {
  auto* native=contents ? contents->GetNativeView() : nullptr;
  auto* host=native ? native->GetRootWindow()->GetHost() : nullptr;
  auto* renderer = host ? Renderer::ForWidget(host->GetAcceleratedWidget()) : nullptr;
  if (!renderer && !std::getenv("CARBONYL_SESSION_SOCKET")) renderer=Renderer::GetCurrent();
  return renderer;
}
void StopBrowserInput(Renderer* renderer) { CloseFramePopup(renderer); clients.erase(renderer); mouse_states.erase(renderer); renderer->Stop(); }
void ObserveAddress(content::WebContents* contents) {
  AddressObserver::CreateForWebContents(contents);
  auto& navigation = contents->GetController();
  if (auto* renderer=RendererForContents(contents)) {
    renderer->PushNav(contents->GetVisibleURL().spec(), navigation.CanGoBack(), navigation.CanGoForward());
    renderer->SetTitle(base::UTF16ToUTF8(contents->GetTitle()));
  }
}

void OpenAddress(content::WebContents* contents, const std::string& address) {
  AddressObserver::CreateForWebContents(contents);
  AddressObserver::FromWebContents(contents)->Open(address);
}

void StartBrowserInput(BrowserInputClient client) {
  auto* renderer=Renderer::GetCurrent();
  clients[renderer] = std::make_unique<BrowserInputClient>(std::move(client));
  std::signal(SIGWINCH, [](int) { resize_pending = 1; });
  static base::NoDestructor<base::RepeatingTimer> resize_timer;
  resize_timer->Start(FROM_HERE, base::Milliseconds(100), base::BindRepeating(&CheckResize));
  carbonyl_renderer_browser_delegate delegate = {
    .shutdown = &OnShutdownInput,
    .refresh = &OnRefreshInput,
    .go_to = &OnGoToInput,
    .go_back = &OnGoBackInput,
    .go_forward = &OnGoForwardInput,
    .scroll = &OnScrollInput,
    .key_press = &OnKeyPressInput,
    .mouse_up = &OnMouseUpInput,
    .mouse_down = &OnMouseDownInput,
    .mouse_move = &OnMouseMoveInput,
    .post_task = [](uintptr_t context, void (*function)(void*), void* data) {
      content::GetUIThreadTaskRunner({})->PostTask(FROM_HERE, base::BindOnce(
        [](uintptr_t context, void (*function)(void*), void* data) {
          Renderer::Scope scope(reinterpret_cast<Renderer*>(context), true);
          function(data);
        }, context, function, data));
    },
    .scroll_to = &ScrollPageTo,
    .dialog_action = [](uint64_t id, int control, int action, const char* value) {
      auto* browser=CurrentClient();
      if (browser && browser->dialog_action)
        browser->dialog_action.Run(id, control, action, value);
    },
    .clipboard_text = [](const char* text, bool query) {
      auto* browser=CurrentClient();
      if (!browser) return;
      if (browser->clipboard_sync) browser->clipboard_sync.Run(text);
      if (!query) {
        // Bracketed paste already carries the exact Unicode text. Insert it
        // into the focused editor without rereading another clipboard buffer.
        if (browser->focus) browser->focus.Run();
        for (auto* contents : browser->pages.Run()) {
          if (auto* frame = contents->GetFocusedFrame()) {
            auto* widget = content::RenderWidgetHostImpl::From(frame->GetRenderWidgetHost());
            if (auto* handler = widget->GetFrameWidgetInputHandler())
              handler->ExecuteEditCommand("InsertText", base::UTF8ToUTF16(text));
          }
        }
      }
    },
    .context = reinterpret_cast<uintptr_t>(renderer),
  };
  Renderer::GetCurrent()->Listen(&delegate);
}
}  // namespace carbonyl
