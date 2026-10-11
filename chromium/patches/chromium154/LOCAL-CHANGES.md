# Local changes on original Carbonyl

Upstream: https://github.com/fathyb/carbonyl (ab80a276b1bd1c2c8dcefc8f248415dfc61dc2bf).
Official Chromium: 154.0.8037.97, b510e9d7cd3a2fbd78d0ddc42234103206c5f78d.

Graphics use Chafa with hhalf, vhalf, quad and sextant only. Native terminal text
is captured separately. Chafa transparency threshold must be 0.5: 0 makes opaque
pixels transparent and returns unset foreground/background colours, causing
visually solid cells despite non-solid Unicode symbols.

Native text capture no longer clears the whole viewport when Skia allocates an
offscreen CSS layer. Paint replay converts save-layer operations into ordinary
save operations for native text capture, retaining transforms and clips. The
separate graphical surface still renders CSS effects normally. The text
regression checks final terminal cells, including all seven yayuuu.pl menu links.

Carbonyl uses Chromium profile storage with persistent session cookies. Bare
addresses try HTTPS first and retry HTTP on transport failure.

YouTube diagnosis: the default HeadlessChrome identity and webdriver marker
cause rejected stream attestation (UMP STREAM_PROTECTION_STATUS 3), initial
buffering stops at about 60 seconds and the player fails around 50 seconds after
navigation. Regular Chrome on the same machine receives protection status 1 and
continues. Changing only the user agent or only the webdriver marker did not
fix Carbonyl. Both together did. Carbonyl now uses a regular Chrome user agent,
Chromium Client Hints, and disables Blink AutomationControlled by default because
it is an interactive browser. The headless-shell product version stays intact.
No media codec was disabled or replaced.

Verification scripts: tests/runtime_smoke.py, tests/browser_integration.py,
tests/text_regression.py. Do not repeat long YouTube playback tests during
routine builds: the user has confirmed playback works and asked to remove
this time-consuming check. The optional manual diagnostic is outside tests,
in scripts/youtube-diagnostic.py, defaults to 20 seconds and has no long-playback
assertion. It records error/protection codes without saving signed streaming
URLs or PO tokens. Run it only when investigating a newly reported media issue.

Build Rust with cargo build --release --locked --target x86_64-unknown-linux-gnu.
Build the full Chromium backend with ninja -C chromium/src/out/Carbonyl chrome -j12.
Stage/archive/install with --backend chrome. The preserved legacy backend uses
headless:headless_shell and the default packaging backend.
Stage/archive/install using scripts/package-local.py. Installation replaces
mapped libraries atomically and retains the previous runtime and launchers.

Google sign-in compatibility investigation: changing UA and webdriver fixes
YouTube playback but does not establish that Google Accounts sign-in is accepted.
A clean-profile comparison with official headed Chrome found that Carbonyl's
Canvas fillText produced zero pixels. The original blanket SkBitmapDevice glyph
suppression also affected web Canvas, not only terminal DOM text. DOM carriers
now have an explicit marker in SkTextBlob UTF-8 metadata; raster suppression
and native capture use that marker, which survives paint-record serialization.
Ordinary Canvas fillText/strokeText now rasterize normally while DOM text stays
native in the terminal. The integration test checks both Canvas drawing APIs.
Before the full-backend migration the engine used headless-shell; missing full
Chrome services (PDF viewer, notification backend and Chrome-specific bindings)
are not represented by fake JavaScript properties. Diagnostic profiles never
read a user's cookies or enter Google account credentials.

Canvas fix verification: the same Arial sample yields 972 nontransparent pixels
in official headed Chrome and the patched Carbonyl (previous Carbonyl: zero).
Canvas strokeText yields 1388 nontransparent pixels. Browser integration passes
including cookies, HTTPS/HTTP fallback and visible Chafa detail. All six CSS
text cases and all seven live yayuuu.pl menu links remain native terminal text.
Google Accounts acceptance after entering an account address is not verified.

The address field now selects its whole URL on clicks anywhere from its opening
bracket through its right edge. Typing or Backspace replaces/removes the whole
selection, arrow keys collapse it and Enter returns focus to the page. Integration
checks replacement from both ends without sending artificial erase sequences.
The Google Accounts rejection still occurs after entering an account address
according to the user's test after the Canvas fix.

Full Chromium backend work: the same official Chromium source now has a Chrome
entrypoint wired to Carbonyl's renderer and common terminal input adapter. It
uses the normal Chrome browser/renderer clients, profile services, PDF viewer,
permission API and Chrome bindings instead of headless-shell. Ozone/headless
provides a display without X/Wayland; the browser itself does not use --headless.
Its fullscreen window is presented through the existing separate graphics and
native DOM text paths. Browser session cookies are explicitly persistent.
Packaging with --backend chrome creates a separate runtime, preserving the
previous headless-shell installation. This backend is not considered validated
until integration, native-text and profile-migration checks pass. Google Accounts
acceptance still requires an actual user sign-in attempt after installation.

The Linux dynamic-launcher portal now respects the existing USE_DBUS build
flag. Without D-Bus it reports the desktop shortcut portal unavailable, allowing
the terminal Chrome target to keep use_dbus=false. Browser web APIs and profile
services still come from the full Chrome implementation.

The common terminal input adapter now supplies DOM key values for printable
characters, arrows, Backspace, Tab and Enter. The integration test types through
the PTY and checks the resulting field value and trusted keydown events. The
full-browser check also verifies that the page fills the terminal viewport.
These are compatibility fixes, not proof of Google Accounts acceptance.

Full Chromium backend verification and installation: the exact staged binary
passes browser integration with native keydown input, Canvas fillText/strokeText,
Chafa symbol detail, native DOM text, HTTPS/HTTP fallback, URL replacement from
both address-field edges, and persistent HTTP-only session cookies. Resize is
checked across a subsequent navigation; native window bounds are resized without
leaving fullscreen. All six CSS text cases and all seven yayuuu.pl menu labels
are present in the final terminal grid. Temporary-profile migration from the
preserved headless-shell engine retains its HTTP-only session cookie.

The Google ServiceLogin diagnostic sees Chrome/154, webdriver=false, five real
PDF plugin entries, valid en-US languages, normal notification permission/query,
and Chrome's own setSyncEncryptionKeys/setClientEncryptionKeys/recovery bindings.
It does not enter an account or establish that Google's server accepts sign-in.
The user must retry sign-in from the beginning after restarting Carbonyl.

For use_dbus=false, Linux desktop file operations use the existing XDG fallback,
notifications use Chromium's message center, disabled Bluetooth is not queried
or shut down, and an unavailable global shortcut listener returns nullptr. The
Linux binary loads libcarbonyl.so from its own runtime directory. Normal startup
URLs are observed without starting the same navigation twice.

The user confirmed that Google sign-in works with the full Chromium backend.
Password saving offers are now disabled through kCredentialsEnableService;
existing password filling and session cookies are retained. Chromium scrollbars
are hidden, and Carbonyl reserves its last terminal column for a native vertical
scrollbar. Track and thumb use only full █ glyphs. Real main-frame scroll offset,
viewport and document heights come through a dedicated Mojo service, and native
track clicks/thumb dragging set the main frame's scroll offset. The service
validates the current visible primary frame and resolves its ID without keeping
a dangling RenderFrameHost pointer. Chafa continues to render graphics only.

Verification: Rust scrollbar geometry/drag tests and renderer tests pass. Browser
integration verifies full-block terminal cells, track clicking to the bottom,
dragging to the top with release outside the column, no leaked DOM clicks, no
Chromium scrollbar gutter, disabled password-save preference, and session-cookie
persistence. CSS text and all seven live yayuuu.pl menu labels remain native.
No YouTube playback or Google login test is repeated for this change.

Portable desktop integration: Carbonyl contains no VTM session tokens, Lua
commands, Elio paths or setup-specific imports. File selection uses the session
org.freedesktop.portal.FileChooser interface. New windows pass the executable,
profile and window ID as separate argv entries to xdg-run, falling back to
xdg-terminal-exec. The external launcher chooses a terminal provider. This
machine's xdg-run selects vtm-terminal through ~/.config/xdg-run/config.json;
that provider and the local portal are independent desktop configuration.

Clipboard text exchange uses OSC 52 writes and queries with strict base64/UTF-8
validation, bounded responses and BEL/ST termination. Bracketed paste is still
supported. The Chromium refresh hook runs in all terminals and preserves its
internal clipboard when a terminal does not answer. Native address/dialog fields
query the same protocol. VTM supports standard OSC 52 queries for the focused
user; a query never overwrites clipboard data.

Mouse text selection is implemented in the Carbonyl terminal renderer, over
the captured native grapheme grid. Page mouse-down is deferred until a release
without dragging; drags never reach Chromium's text-selection layer. Ordinary
clicks, frame controls and the native scrollbar retain their input routing.
Selection rendering temporarily highlights cells without altering the stored
Chafa/native text frame. Copy extracts only Unicode text graphemes, ignoring
raster symbols; Ctrl+C exports it through OSC 52 and updates Chromium's internal
clipboard, while Escape or the next click clears the selection.

Local regression tests, diagnostics, logs and build artifacts are ignored by
Git. The packaging helper verifies staged file hashes without depending on the
local test directory; --require-tested additionally checks local integration
receipts when installing on the development machine.
