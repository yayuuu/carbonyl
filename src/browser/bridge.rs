use std::ffi::{CStr, CString};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::{env, io, thread};

use libc::{c_char, c_float, c_int, c_uchar, c_uint, c_void, size_t};

use crate::cli::{CommandLine, CommandLineProgram, EnvVar};
use crate::gfx::{Cast, Color, Point, Rect, Size};
use crate::output::{RenderThread, Window};
use crate::ui::navigation::NavigationAction;
use crate::{input, utils::log};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CSize {
    width: c_uint,
    height: c_uint,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CPoint {
    x: c_uint,
    y: c_uint,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CRect {
    origin: CPoint,
    size: CSize,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CColor {
    r: u8,
    g: u8,
    b: u8,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CText {
    text: *const c_char,
    rect: CRect,
    color: CColor,
}

#[repr(C)]
pub struct RendererBridge {
    cmd: CommandLine,
    window: Window,
    renderer: RenderThread,
    stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

unsafe impl Send for RendererBridge {}
unsafe impl Sync for RendererBridge {}

pub type RendererPtr = *const Mutex<RendererBridge>;

impl<T: Copy> From<CPoint> for Point<T>
where
    c_uint: Cast<T>,
{
    fn from(value: CPoint) -> Self {
        Point::new(value.x, value.y).cast()
    }
}
impl From<Size<c_uint>> for CSize {
    fn from(value: Size<c_uint>) -> Self {
        Self {
            width: value.width,
            height: value.height,
        }
    }
}
impl<T: Copy> From<CSize> for Size<T>
where
    c_uint: Cast<T>,
{
    fn from(value: CSize) -> Self {
        Size::new(value.width, value.height).cast()
    }
}
impl From<CColor> for Color {
    fn from(value: CColor) -> Self {
        Color::new(value.r, value.g, value.b)
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct BrowserDelegate {
    shutdown: extern "C" fn(),
    refresh: extern "C" fn(),
    go_to: extern "C" fn(*const c_char),
    go_back: extern "C" fn(),
    go_forward: extern "C" fn(),
    scroll: extern "C" fn(c_int),
    key_press: extern "C" fn(c_char),
    mouse_up: extern "C" fn(c_uint, c_uint),
    mouse_down: extern "C" fn(c_uint, c_uint),
    mouse_move: extern "C" fn(c_uint, c_uint),
    post_task: extern "C" fn(usize, extern "C" fn(*mut c_void), *mut c_void),
    scroll_to: extern "C" fn(c_float),
    dialog_action: extern "C" fn(u64, c_int, c_int, *const c_char),
    clipboard_text: extern "C" fn(*const c_char, bool),
    context:usize,
}

fn main() -> io::Result<Option<i32>> {
    let cmd = match CommandLineProgram::parse_or_run() {
        None => return Ok(Some(0)),
        Some(cmd) => cmd,
    };

    if cmd.shell_mode {
        return Ok(None);
    }
    if env::var("CARBONYL_CHROME_BACKEND").as_deref() == Ok("1") {
        return super::session::run(cmd).map(Some);
    }

    let mut terminal = input::Terminal::setup();
    let mut command = Command::new(env::current_exe()?);
    // Carbonyl transfers the graphics-only software surface to the terminal.
    command.arg("--disable-gpu-compositing").arg("--hide-scrollbars");
    if env::var("CARBONYL_CHROME_BACKEND").as_deref() == Ok("1") {
        let window = Window::read();
        let width = (window.browser.width as f32 * window.dpi).ceil() as u32;
        let height = (window.browser.height as f32 * window.dpi).ceil() as u32;
        command
            .arg("--ozone-platform=headless")
            .arg(format!("--ozone-override-screen-size={},{}", width, height))
            .arg("--kiosk")
            .arg("--start-fullscreen")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--password-store=basic")
            .arg("--use-angle=swiftshader");
    }

    // Carbonyl is an interactive browser. headless_shell otherwise advertises
    // navigator.webdriver=true, which breaks YouTube playback attestation.
    command.arg("--disable-blink-features=AutomationControlled");

    if !cmd.args.iter().any(|arg| arg == "--user-data-dir" || arg.starts_with("--user-data-dir=")) {
        let profile = env::var_os("CARBONYL_PROFILE_DIR").map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                let data = env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from(env::var_os("HOME").unwrap_or_default())
                        .join(".local/share"));
                data.join("carbonyl/profile")
            });
        std::fs::create_dir_all(&profile)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700))?;
        command.arg(format!("--user-data-dir={}", profile.display()));
    }

    if !cmd.bitmap {
        command
            .arg("--disable-threaded-scrolling")
            .arg("--disable-threaded-animation");
    }

    let output = command
        .args(cmd.args)
        .env(EnvVar::ShellMode, "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .output()?;

    terminal.teardown();

    let code = output.status.code().unwrap_or(127);

    if code != 0 || cmd.debug {
        io::stderr().write_all(&output.stderr)?;
    }

    Ok(Some(code))
}

#[no_mangle]
pub extern "C" fn carbonyl_bridge_main() {
    match main() {
        Ok(Some(code))=>std::process::exit(code),
        Ok(None)=>(),
        Err(error)=>{eprintln!("Carbonyl: {error}");std::process::exit(1);}
    }
}

#[no_mangle]
pub extern "C" fn carbonyl_bridge_bitmap_mode() -> bool {
    CommandLine::parse().bitmap
}

#[no_mangle]
pub extern "C" fn carbonyl_bridge_get_dpi() -> c_float {
    Window::read().dpi
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_create() -> RendererPtr { carbonyl_renderer_create_terminal(1) }
#[no_mangle]
pub extern "C" fn carbonyl_renderer_create_terminal(fd:c_int) -> RendererPtr {
    let bridge = RendererBridge {
        cmd: CommandLine::parse(),
        window: Window::read_fd(fd),
        renderer: RenderThread::new_fd(fd),
        stopped:std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    Box::into_raw(Box::new(Mutex::new(bridge)))
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_stop(bridge:RendererPtr) {
    let bridge=unsafe {&*bridge};
    let handle={let mut b=bridge.lock().unwrap();
        b.stopped.store(true,std::sync::atomic::Ordering::Relaxed);b.renderer.stop()};
    if let Some(handle)=handle {let _=handle.join();}
}
#[no_mangle]
pub extern "C" fn carbonyl_renderer_changed(bridge:RendererPtr)->bool {
    let bridge=unsafe {&*bridge}; let mut b=bridge.lock().unwrap();
    let old=b.window.cells; b.window.update(); old!=b.window.cells
}
#[no_mangle]
pub extern "C" fn carbonyl_renderer_start(bridge: RendererPtr) {
    {
        let bridge = unsafe { bridge.as_ref() };
        let mut bridge = bridge.unwrap().lock().unwrap();

        bridge.renderer.enable()
    }

    carbonyl_renderer_resize(bridge);
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_resize(bridge: RendererPtr) {
    let bridge = unsafe { bridge.as_ref() };
    let mut bridge = bridge.unwrap().lock().unwrap();
    let window = bridge.window.update();
    let cells = window.cells.clone();

    log::debug!("resizing renderer, terminal window: {:?}", window);

    bridge
        .renderer
        .render(move |renderer| renderer.set_size(cells));
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_push_nav(
    bridge: RendererPtr,
    url: *const c_char,
    can_go_back: bool,
    can_go_forward: bool,
) {
    let (bridge, url) = unsafe { (bridge.as_ref(), CStr::from_ptr(url)) };
    let (mut bridge, url) = (bridge.unwrap().lock().unwrap(), url.to_owned());

    bridge.renderer.render(move |renderer| {
        renderer.push_nav(url.to_str().unwrap(), can_go_back, can_go_forward)
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_set_title(bridge: RendererPtr, title: *const c_char) {
    let (bridge, title) = unsafe { (bridge.as_ref(), CStr::from_ptr(title)) };
    let (mut bridge, title) = (bridge.unwrap().lock().unwrap(), title.to_owned());

    bridge
        .renderer
        .render(move |renderer| renderer.set_title(title.to_str().unwrap()).unwrap());
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_draw_text(
    bridge: RendererPtr,
    text: *const CText,
    text_size: size_t,
) {
    let (bridge, text) = unsafe { (bridge.as_ref(), std::slice::from_raw_parts(text, text_size)) };
    let mut bridge = bridge.unwrap().lock().unwrap();
    let mut vec = text
        .iter()
        .map(|text| {
            let str = unsafe { CStr::from_ptr(text.text) };

            (
                str.to_str().unwrap().to_owned(),
                text.rect.origin.into(),
                text.rect.size.into(),
                text.color.into(),
            )
        })
        .collect::<Vec<(String, Point, Size, Color)>>();

    bridge.renderer.render(move |renderer| {
        renderer.clear_text();

        for (text, origin, size, color) in std::mem::take(&mut vec) {
            renderer.draw_text(&text, origin, size, color)
        }
    });
}

#[derive(Clone, Copy)]
struct CallbackData(*const c_void);

impl CallbackData {
    pub fn as_ptr(&self) -> *const c_void {
        self.0
    }
}

unsafe impl Send for CallbackData {}
unsafe impl Sync for CallbackData {}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_draw_bitmap(
    bridge: RendererPtr,
    pixels: *const c_uchar,
    pixels_size: CSize,
    rect: CRect,
    callback: extern "C" fn(*const c_void),
    callback_data: *const c_void,
) {
    let length = (pixels_size.width * pixels_size.height * 4) as usize;
    let (bridge, pixels) = unsafe { (bridge.as_ref(), std::slice::from_raw_parts(pixels, length)) };
    let callback_data = CallbackData(callback_data);
    let mut bridge = bridge.unwrap().lock().unwrap();

    bridge.renderer.render(move |renderer| {
        renderer.draw_background(
            pixels,
            pixels_size.into(),
            Rect {
                size: rect.size.into(),
                origin: rect.origin.into(),
            },
        );

        callback(callback_data.as_ptr());
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_update_scroll(bridge: RendererPtr, offset: c_float, viewport: c_float, content: c_float) {
    let bridge = unsafe { bridge.as_ref() }.unwrap();
    bridge.lock().unwrap().renderer.render(move |renderer| renderer.update_scroll(offset, viewport, content));
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_download_status(bridge: RendererPtr, text: *const c_char) {
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned();
    unsafe { bridge.as_ref() }.unwrap().lock().unwrap().renderer.render(move |renderer| {
        renderer.set_download_status(text.clone());
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_clipboard_write(bridge:RendererPtr,text:*const c_char) {
    let encoded=unsafe{CStr::from_ptr(text)}.to_string_lossy().into_owned();
    unsafe{bridge.as_ref()}.unwrap().lock().unwrap().renderer.render(move|renderer|{
        renderer.write_clipboard(&encoded).unwrap();
    });
}
#[no_mangle]
pub extern "C" fn carbonyl_renderer_clipboard_query(bridge:RendererPtr) {
    unsafe{bridge.as_ref()}.unwrap().lock().unwrap().renderer.render(|renderer|{
        renderer.query_clipboard().unwrap();
    });
}
#[repr(C)]
pub struct CDialogControl {
    id:c_int, kind:c_int, label:*const c_char, value:*const c_char, flags:c_uint,
}
#[no_mangle]
pub extern "C" fn carbonyl_renderer_show_dialog(bridge:RendererPtr,id:u64,title:*const c_char,
    controls:*const CDialogControl,count:size_t) {
    let title=unsafe { CStr::from_ptr(title) }.to_string_lossy().into_owned();
    let controls=if count==0 {Vec::new()} else {
        unsafe { std::slice::from_raw_parts(controls,count) }.iter().map(|c|crate::ui::dialog::Control{
            id:c.id,kind:c.kind,flags:c.flags,
            label:unsafe {CStr::from_ptr(c.label)}.to_string_lossy().into_owned(),
            value:unsafe {CStr::from_ptr(c.value)}.to_string_lossy().into_owned(),
        }).collect()
    };
    let mut controls=Some(controls);
    unsafe {bridge.as_ref()}.unwrap().lock().unwrap().renderer.render(move|renderer|{
        renderer.update_dialog(id,title.clone(),controls.take().unwrap());
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_get_size(bridge: RendererPtr) -> CSize {
    let bridge = unsafe { bridge.as_ref() };
    let bridge = bridge.unwrap().lock().unwrap();

    log::debug!("terminal size: {:?}", bridge.window.browser);

    bridge.window.browser.into()
}

extern "C" fn post_task_handler(callback: *mut c_void) {
    let mut closure = unsafe { Box::from_raw(callback as *mut Box<dyn FnMut()>) };

    closure()
}

unsafe fn post_task<F>(context:usize, handle: extern "C" fn(usize, extern "C" fn(*mut c_void), *mut c_void), run: F)
where
    F: FnMut() + Send + 'static,
{
    let closure: *mut Box<dyn FnMut()> = Box::into_raw(Box::new(Box::new(run)));

    handle(context, post_task_handler, closure as *mut c_void);
}

/// Function called by the C++ code to listen for input events.
///
/// This will block so the calling code should start and own a dedicated thread.
/// It will panic if there is any error.
#[no_mangle]
pub extern "C" fn carbonyl_renderer_listen(bridge: RendererPtr, delegate: *mut BrowserDelegate) {
    let bridge = unsafe { &*bridge };
    let delegate = unsafe { *delegate };

    use input::*;

    let (fd,stop)={let b=bridge.lock().unwrap();(b.window.fd,b.stopped.clone())};
    thread::spawn(move || {
        macro_rules! emit {
            ($event:ident($($args:expr),*) => $closure:expr) => {{
                let run = move || {
                    (delegate.$event)($($args),*);

                    $closure
                };

                unsafe { post_task(delegate.context, delegate.post_task, run) }
            }};
            ($event:ident($($args:expr),*)) => {{
                emit!($event($($args),*) => {})
            }};
        }

        let _=listen_fd(if fd==1 {0}else{fd},&stop,|mut events| {
            bridge.lock().unwrap().renderer.render(move |renderer| {
                let get_scale = || bridge.lock().unwrap().window.scale;
                let scale = |col, row| {
                    let scale = get_scale();

                    scale
                        .mul(((col as f32 + 0.5), (row as f32 - 0.5)))
                        .floor()
                        .cast()
                        .into()
                };
                let dispatch = |action| {
                    match action {
                        NavigationAction::Ignore => (),
                        NavigationAction::DialogAction(id, control, action, value) => {
                            let value=CString::new(value).unwrap();
                            emit!(dialog_action(id,control,action,value.as_ptr()))
                        },
                        NavigationAction::Paste(text)=> {
                            let text=CString::new(text.replace('\0',"")).unwrap();
                            emit!(clipboard_text(text.as_ptr(),false))
                        },
                        NavigationAction::Click(point) => {
                            let (width, height) = scale(point.x as usize, point.y as usize);
                            emit!(mouse_down(width, height));
                            emit!(mouse_up(width, height));
                        },
                        NavigationAction::ScrollTo(offset) => emit!(scroll_to(offset)),
                        NavigationAction::Forward => return true,
                        NavigationAction::GoBack() => emit!(go_back()),
                        NavigationAction::GoForward() => emit!(go_forward()),
                        NavigationAction::Refresh() => emit!(refresh()),
                        NavigationAction::GoTo(url) => {
                            let c_str = CString::new(url).unwrap();

                            emit!(go_to(c_str.as_ptr()))
                        }
                    };

                    return false;
                };

                for event in std::mem::take(&mut events) {
                    use Event::*;

                    match event {
                        Clipboard(text) => {
                            let copy = CString::new(text.replace('\0', "")).unwrap();
                            emit!(clipboard_text(copy.as_ptr(), true));
                            dispatch(renderer.clipboard_reply(&text));
                        },
                        Paste { text } => {
                            let text = text.replace('\0', "");
                            let copy = CString::new(text.clone()).unwrap();
                            emit!(clipboard_text(copy.as_ptr(), true));
                            dispatch(renderer.paste(&text));
                        },
                        Exit => {
                            if renderer.has_selection() {
                                if let Some(text) = renderer.copy_selection().unwrap() {
                                    let copy = CString::new(text.replace('\0', "")).unwrap();
                                    emit!(clipboard_text(copy.as_ptr(), true));
                                }
                            } else if renderer.dialog_open() {
                                dispatch(renderer.keypress(&Key::from(0x1b)).unwrap());
                            } else { emit!(shutdown()); }
                        },
                        Scroll { delta } => {
                            if renderer.dialog_open() { continue; }
                            let scale = get_scale();

                            emit!(scroll((delta as f32 * scale.height) as c_int))
                        }
                        KeyPress { key } => {
                            if dispatch(renderer.keypress(&key).unwrap()) {
                                emit!(key_press(key.char as c_char))
                            }
                        }
                        MouseUp { col, row } => {
                            if dispatch(renderer.mouse_up((col as _, row as _).into()).unwrap()) {
                                let (width, height) = scale(col, row);

                                emit!(mouse_up(width, height))
                            }
                        }
                        MouseDown { col, row } => {
                            if dispatch(renderer.mouse_down((col as _, row as _).into()).unwrap()) {
                                let (width, height) = scale(col, row);

                                emit!(mouse_down(width, height))
                            }
                        }
                        MouseMove { col, row } => {
                            if dispatch(renderer.mouse_move((col as _, row as _).into()).unwrap()) {
                                let (width, height) = scale(col, row);

                                emit!(mouse_move(width, height))
                            }
                        }
                        Terminal(terminal) => match terminal {
                            TerminalEvent::Name(name) => log::debug!("terminal name: {name}"),
                            TerminalEvent::TrueColorSupported => renderer.enable_true_color(),
                        },
                    }
                }
            })
        });
        if stop.load(std::sync::atomic::Ordering::Relaxed) {return;}

        // Setup single-use channel
        let (tx, rx) = mpsc::channel();

        // Signal the browser to shutdown and notify our thread
        emit!(shutdown() => tx.send(()).unwrap());
        rx.recv().unwrap();

        // Shutdown rendering thread
        // if let Some(handle) = { bridge.lock().unwrap().renderer().stop() } {
        //     handle.join().unwrap()
        // }
    });
}
