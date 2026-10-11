// Each terminal is a frontend of one real Chromium process/profile. The PTY
// descriptor is transferred over a private, same-user Unix socket, never copied.
use std::{env, fs::{self,OpenOptions}, io::{self,Read,Write}, os::{fd::AsRawFd,unix::{fs::{OpenOptionsExt,PermissionsExt},net::UnixStream,process::CommandExt}}, path::PathBuf, process::{Command,Stdio}, thread, time::{Duration,Instant}};
use crate::{cli::{CommandLine,EnvVar},input::Terminal,output::Window};
pub fn run(cmd:CommandLine)->io::Result<i32> {
    let mut profile=None;let mut address=String::from("about:blank");let mut window_id=0u64;
    let mut engine_args=Vec::new();let mut it=cmd.args.iter();
    while let Some(arg)=it.next() {
        if arg=="--user-data-dir" {profile=it.next().map(PathBuf::from);}
        else if let Some(p)=arg.strip_prefix("--user-data-dir="){profile=Some(PathBuf::from(p));}
        else if let Some(id)=arg.strip_prefix("--carbonyl-window-id="){window_id=id.parse().unwrap_or(0);}
        else if arg.starts_with('-') {engine_args.push(arg.clone());}
        else {address=arg.clone();}
    }
    let profile=profile.or_else(||env::var_os("CARBONYL_PROFILE_DIR").map(PathBuf::from)).unwrap_or_else(||{
        env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/share")).join("carbonyl/profile")
    });
    fs::create_dir_all(&profile)?;let profile=fs::canonicalize(profile)?;
    fs::set_permissions(&profile,fs::Permissions::from_mode(0o700))?;
    let lock=OpenOptions::new().create(true).read(true).write(true).mode(0o600).open(profile.join(".carbonyl-launch.lock"))?;
    if unsafe {libc::flock(lock.as_raw_fd(),libc::LOCK_EX)}!=0 {return Err(io::Error::last_os_error());}
    let socket=profile.join(".carbonyl-session.sock");
    let mut child=None;
    let mut stream=match UnixStream::connect(&socket) {
        Ok(s)=>s,
        Err(_)=>{
            if let Ok(target)=fs::read_link(profile.join("SingletonLock")) {
                if let Some(pid)=target.to_string_lossy().rsplit('-').next().and_then(|s|s.parse::<i32>().ok()) {
                    if unsafe {libc::kill(pid,0)}==0 {return Err(io::Error::new(io::ErrorKind::AlreadyExists,"Zamknij wszystkie okna wcześniejszej wersji Carbonyl i uruchom ponownie."));}
                }
            }
            if socket.exists(){fs::remove_file(&socket)?;}
            let w=Window::read();let pixels=w.browser.cast::<f32>().mul(w.dpi).ceil().cast::<u32>();
            let log=OpenOptions::new().append(true).create(true).mode(0o600).open(profile.join("carbonyl-engine.log"))?;
            let mut command=Command::new(env::current_exe()?);
            command.args(["--disable-gpu-compositing","--hide-scrollbars","--disable-blink-features=AutomationControlled","--ozone-platform=headless","--kiosk","--start-fullscreen","--no-first-run","--no-default-browser-check","--password-store=basic","--use-angle=swiftshader"])
                .arg(format!("--ozone-override-screen-size={},{}",pixels.width,pixels.height))
                .arg(format!("--user-data-dir={}",profile.display())).args(engine_args).arg("about:blank")
                .env(EnvVar::ShellMode,"1").env("CARBONYL_SESSION_SOCKET",&socket)
                .env("COLUMNS",w.cells.width.to_string()).env("LINES",(w.cells.height+1).to_string())
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(log));
            if !cmd.bitmap {command.args(["--disable-threaded-scrolling","--disable-threaded-animation"]);}
            unsafe {command.pre_exec(||{if libc::setsid()<0 {return Err(io::Error::last_os_error());} Ok(())});}
            child=Some(command.spawn()?);
            let deadline=Instant::now()+Duration::from_secs(30);
            loop {match UnixStream::connect(&socket){Ok(s)=>break s,Err(e)=>{
                if child.as_mut().unwrap().try_wait()?.is_some() || Instant::now()>deadline{return Err(io::Error::new(e.kind(),format!("Silnik Carbonyl nie uruchomił się. Log: {}",profile.join("carbonyl-engine.log").display())));}
                thread::sleep(Duration::from_millis(50));
            }}}
        }
    };
    let mut terminal=Terminal::setup();
    if address.len()>65536 {return Err(io::Error::new(io::ErrorKind::InvalidInput,"Adres jest zbyt długi"));}
    let mut header=Vec::from(*b"CBTW0001");header.extend_from_slice(&(address.len() as u32).to_le_bytes());header.extend_from_slice(&window_id.to_le_bytes());
    unsafe {
        let mut control=[0usize;4];let mut msg:libc::msghdr=std::mem::zeroed();
        let mut iov=libc::iovec{iov_base:header.as_mut_ptr().cast(),iov_len:header.len()};
        msg.msg_iov=&mut iov;msg.msg_iovlen=1;msg.msg_control=control.as_mut_ptr().cast();msg.msg_controllen=libc::CMSG_SPACE(4) as usize;
        let c=libc::CMSG_FIRSTHDR(&msg);(*c).cmsg_level=libc::SOL_SOCKET;(*c).cmsg_type=libc::SCM_RIGHTS;(*c).cmsg_len=libc::CMSG_LEN(4) as usize;
        *(libc::CMSG_DATA(c).cast::<i32>())=0;
        let sent=libc::sendmsg(stream.as_raw_fd(),&msg,libc::MSG_NOSIGNAL);
        if sent<0{return Err(io::Error::last_os_error());} stream.write_all(&header[sent as usize..])?;
    }
    stream.write_all(address.as_bytes())?;stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut response=[0u8];stream.read_exact(&mut response)?;
    if response[0]!=b'O' {return Err(io::Error::new(io::ErrorKind::Other,"Nie udało się otworzyć okna Carbonyl"));}
    unsafe {libc::flock(lock.as_raw_fd(),libc::LOCK_UN);}
    stream.set_read_timeout(None)?;
    let last=stream.read(&mut response).unwrap_or(0)>0 && response[0]==b'L';
    terminal.teardown();
    if last {
        let deadline=Instant::now()+Duration::from_secs(10);
        while profile.join("SingletonLock").symlink_metadata().is_ok() && Instant::now()<deadline {thread::sleep(Duration::from_millis(30));}
        if let Some(mut c)=child {if c.try_wait()?.is_none() && !profile.join("SingletonLock").symlink_metadata().is_ok(){let _=c.wait();}}
    }
    Ok(0)
}
