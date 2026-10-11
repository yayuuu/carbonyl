#include "carbonyl/src/browser/browser_input.h"

#include "base/functional/bind.h"
#include "headless/lib/browser/headless_web_contents_impl.h"
#include "headless/public/headless_browser.h"
#include "headless/public/headless_browser_context.h"
#include "ui/gfx/geometry/rect.h"

namespace carbonyl {
void OpenAddress(headless::HeadlessWebContents* contents, const std::string& address) {
  OpenAddress(headless::HeadlessWebContentsImpl::From(contents)->web_contents(), address);
}
void StartBrowserInput(headless::HeadlessBrowser* active) {
  BrowserInputClient client;
  client.pages = base::BindRepeating([](headless::HeadlessBrowser* active) {
    std::vector<content::WebContents*> pages;
    for (auto* context : active->GetAllBrowserContexts())
      for (auto* page : context->GetAllWebContents())
        pages.push_back(headless::HeadlessWebContentsImpl::From(page)->web_contents());
    return pages;
  }, base::Unretained(active));
  client.resize = base::BindRepeating([](headless::HeadlessBrowser* active, gfx::Size size) {
    for (auto* context : active->GetAllBrowserContexts())
      for (auto* page : context->GetAllWebContents())
        headless::HeadlessWebContentsImpl::From(page)->SetBounds(gfx::Rect(size));
  }, base::Unretained(active));
  client.shutdown = base::BindRepeating(&headless::HeadlessBrowser::Shutdown, base::Unretained(active));
  StartBrowserInput(std::move(client));
}
}  // namespace carbonyl
