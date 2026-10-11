use std::io::{self, Read};
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicBool,Ordering};

use crate::input::*;

/// Listen for input events in stdin.
/// This will block, so it should run from a dedicated thread.
pub fn listen<F>(callback: F) -> io::Result<()>
where
    F: FnMut(Vec<Event>),
{
    listen_fd(0,&AtomicBool::new(false),callback)
}
pub fn listen_fd<F>(fd:i32, stop:&AtomicBool, mut callback:F)->io::Result<()> where F:FnMut(Vec<Event>) {
    let mut buf = [0u8; 1024];
    let copy=unsafe {libc::dup(fd)};
    if copy<0 {return Err(io::Error::last_os_error());}
    let mut stdin = unsafe {std::fs::File::from_raw_fd(copy)};
    let mut parser = Parser::new();

    loop {
        if stop.load(Ordering::Relaxed) {return Ok(());}
        let mut poll=libc::pollfd{fd:copy,events:libc::POLLIN,revents:0};
        if unsafe {libc::poll(&mut poll,1,100)}<=0 {continue;}
        // Wait for some input
        let size = stdin.read(&mut buf)?;
        if size==0 { return Ok(()); }
        let mut read = parser.parse(&buf[0..size]);
        if parser.pending_escape() {
            let mut poll=libc::pollfd{fd:copy,events:libc::POLLIN,revents:0};
            if unsafe {libc::poll(&mut poll,1,30)}==0 {read.extend(parser.flush_escape());}
        }
        let mut scroll = 0;
        let mut events = Vec::with_capacity(read.len());

        for event in read {
            match event {
                Event::Scroll { delta } => scroll += delta,
                event => events.push(event),
            }
        }

        if scroll != 0 {
            events.push(Event::Scroll { delta: scroll })
        }

        callback(events)
    }
}
