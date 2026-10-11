#ifndef CARBONYL_SRC_BROWSER_BROWSER_INPUT_H_
#define CARBONYL_SRC_BROWSER_BROWSER_INPUT_H_

#include <cstdint>
#include <string>
#include <vector>
#include "base/functional/callback.h"
#include "ui/gfx/geometry/size.h"
#include "url/gurl.h"

namespace content { class WebContents; }
namespace headless {
class HeadlessBrowser;
class HeadlessWebContents;
}

namespace carbonyl {
class Renderer;
Renderer* RendererForContents(content::WebContents* contents);
void StopBrowserInput(Renderer* renderer);
struct BrowserInputClient {
  base::RepeatingCallback<std::vector<content::WebContents*>()> pages;
  base::RepeatingCallback<void(gfx::Size)> resize;
  base::RepeatingClosure shutdown;
  base::RepeatingClosure focus;
  base::RepeatingCallback<void(const std::string&)> clipboard_sync;
  base::RepeatingCallback<void(uint64_t, int, int, const std::string&)> dialog_action;
};
void StartBrowserInput(BrowserInputClient client);
void ObserveAddress(content::WebContents* contents);
void OpenAddress(content::WebContents* contents, const std::string& address);
void StartBrowserInput(headless::HeadlessBrowser* browser);
void OpenAddress(headless::HeadlessWebContents* contents, const std::string& address);
void StartChromeBrowserInput();
}

#endif
