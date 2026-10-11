use std::ops::ControlFlow;

use crate::input::*;

#[derive(Default)]
pub struct Parser {
    events: Vec<Event>,
    sequence: Sequence,
}

#[derive(Default)]
enum Sequence {
    #[default]
    Char,
    Escape,
    Control,
    Mouse(Mouse),
    DeviceControl(DeviceControl),
    Csi(Vec<u8>),
    Apc(bool),
    Osc(Vec<u8>, bool, bool),
    Paste(Vec<u8>),
    DiscardPaste(Vec<u8>),
}

#[derive(Clone, Debug)]
pub enum TerminalEvent {
    Name(String),
    TrueColorSupported,
}

#[derive(Clone, Debug)]
pub enum Event {
    KeyPress { key: Key },
    MouseUp { row: usize, col: usize },
    MouseDown { row: usize, col: usize },
    MouseMove { row: usize, col: usize },
    Scroll { delta: isize },
    Terminal(TerminalEvent),
    Exit,
    Paste { text: String },
    Clipboard(String),
}

pub type ParseControlFlow = ControlFlow<Option<Event>, Option<Event>>;

impl Parser {
    pub fn new() -> Parser {
        Self::default()
    }

    pub fn pending_escape(&self)->bool {matches!(self.sequence,Sequence::Escape)}
    pub fn flush_escape(&mut self)->Vec<Event> {
        if self.pending_escape() {self.sequence=Sequence::Char;vec![Event::KeyPress{key:0x1b.into()}]}
        else {Vec::new()}
    }
    pub fn parse(&mut self, input: &[u8]) -> Vec<Event> {
        let mut sequence = std::mem::take(&mut self.sequence);

        macro_rules! emit {
            ($event:expr) => {{
                if let Some(event) = $event.into() {
                    self.events.push(event);
                }

                Sequence::Char
            }};
            ($event:expr; continue) => {{
                if let Some(event) = $event.into() {
                    self.events.push(event);
                }

                continue;
            }};
        }
        macro_rules! parse {
            ($parser:expr, $key:expr) => (
                match $parser.parse($key) {
                    ControlFlow::Break(None) => Sequence::Char,
                    ControlFlow::Break(Some(event)) => emit!(event),
                    ControlFlow::Continue(None) => continue,
                    ControlFlow::Continue(Some(event)) => emit!(event; continue),
                }
            );
        }

        for &key in input {
            sequence = match sequence {
                Sequence::Char => match key {
                    0x1b => Sequence::Escape,
                    0x03 => emit!(Event::Exit),
                    key => emit!(Event::KeyPress { key: key.into() }),
                },
                Sequence::Escape => match key {
                    b'[' => Sequence::Control,
                    b'P' => Sequence::DeviceControl(DeviceControl::new()),
                    b'_' => Sequence::Apc(false),
                    b']' => Sequence::Osc(Vec::new(), false, false),
                    0x1b => emit!(Event::KeyPress { key: 0x1b.into() }; continue),
                    key => {
                        emit!(Event::KeyPress { key: 0x1b.into() });
                        emit!(Event::KeyPress { key: key.into() })
                    }
                },
                Sequence::Control => match key {
                    b'<' => Sequence::Mouse(Mouse::new()),
                    b'0'..=b'9' => Sequence::Csi(vec![key]),
                    key => emit!(Keyboard::key(key, 0)),
                },
                Sequence::Mouse(ref mut mouse) => parse!(mouse, key),
                Sequence::Csi(ref mut bytes) => {
                    bytes.push(key);
                    if key.is_ascii_alphabetic() || key==b'~' {
                        match bytes.as_slice() {
                            b"200~" => Sequence::Paste(Vec::new()),
                            value if key.is_ascii_alphabetic() => {
                                let modifier=value.split(|b|*b==b';').nth(1)
                                    .and_then(|v| std::str::from_utf8(&v[..v.len()-1]).ok())
                                    .and_then(|v|v.parse::<u8>().ok()).unwrap_or(0);
                                emit!(Keyboard::key(key,modifier))
                            },
                            _=>Sequence::Char,
                        }
                    } else if bytes.len()>32 {Sequence::Char} else {continue;}
                },
                Sequence::Apc(ref mut escape) => {
                    if key == b'\\' && *escape { Sequence::Char }
                    else { *escape = key == 0x1b; continue; }
                },
                Sequence::Osc(ref mut bytes, ref mut escape, ref mut oversized) => {
                    if key == 0x07 || key == b'\\' && *escape {
                        if !*oversized {
                            if let Some(text) = clipboard_response(bytes) {
                                self.events.push(Event::Clipboard(text));
                            }
                        }
                        Sequence::Char
                    } else {
                        if *escape && !*oversized { bytes.push(0x1b); }
                        *escape = key == 0x1b;
                        if !*escape && !*oversized {
                            if bytes.len() < 24 * 1024 * 1024 { bytes.push(key); }
                            else { bytes.clear(); *oversized = true; }
                        }
                        continue;
                    }
                },
                Sequence::Paste(ref mut bytes) => {
                    bytes.push(key);
                    let end=b"\x1b[201~".as_slice();
                    if bytes.ends_with(end) {
                        bytes.truncate(bytes.len()-end.len());
                        let text=String::from_utf8_lossy(bytes).into_owned();
                        emit!(Event::Paste{text})
                    } else if bytes.len()>16*1024*1024 {Sequence::DiscardPaste(Vec::new())} else {continue;}
                },
                Sequence::DiscardPaste(ref mut bytes) => {
                    bytes.push(key);
                    let end=b"\x1b[201~".as_slice();
                    if bytes.ends_with(end) {Sequence::Char} else {
                        if bytes.len()>end.len() {bytes.remove(0);} continue;
                    }
                },
                Sequence::DeviceControl(ref mut dcs) => parse!(dcs, key),
            }
        }

        self.sequence = sequence;

        std::mem::take(&mut self.events)
    }
}


// OSC 52 text is encoded, never replayed as terminal keystrokes. Reject malformed
// base64 and incomplete UTF-8 rather than importing corrupt clipboard content.
fn clipboard_response(bytes: &[u8]) -> Option<String> {
    let tail = bytes.strip_prefix(b"52;")?;
    let separator = tail.iter().position(|&byte| byte == b';')?;
    let selections = &tail[..separator];
    if !selections.is_empty() && !selections.iter().any(|b| b"cpqs01234567".contains(b)) {
        return None;
    }
    let encoded = &tail[separator + 1..];
    if encoded.len() % 4 != 0 || encoded.len() > 24 * 1024 * 1024 { return None; }
    let mut result = Vec::with_capacity(encoded.len() / 4 * 3);
    fn value(byte: u8) -> Option<u8> {
        Some(match byte {
            b'A'..=b'Z' => byte - b'A', b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52, b'+' => 62, b'/' => 63,
            _ => return None,
        })
    }
    for (i, group) in encoded.chunks_exact(4).enumerate() {
        let a = value(group[0])?;
        let b = value(group[1])?;
        let last = (i + 1) * 4 == encoded.len();
        result.push(a << 2 | b >> 4);
        if group[2] == b'=' {
            if !last || group[3] != b'=' || b & 15 != 0 { return None; }
        } else {
            let c = value(group[2])?;
            result.push(b << 4 | c >> 2);
            if group[3] == b'=' {
                if !last || c & 3 != 0 { return None; }
            } else { result.push(c << 6 | value(group[3])?); }
        }
    }
    if result.len() > 16 * 1024 * 1024 { return None; }
    String::from_utf8(result).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn osc52_reply_is_one_unicode_event_and_does_not_execute_controls() {
        // "żółć\n" followed by Ctrl+C and an arrow escape sequence.
        let mut parser = Parser::new();
        let reply = b"\x1b]52;c;xbzDs8WCxIcKAxtbQQ==\x1b\\";
        let mut events = Vec::new();
        for byte in reply { events.extend(parser.parse(&[*byte])); }
        assert!(matches!(events.as_slice(), [Event::Clipboard(t)] if t == "żółć\n\u{3}\u{1b}[A"));
    }
    #[test]
    fn osc52_accepts_bel_empty_selection_and_empty_clipboard() {
        let mut parser = Parser::new();
        assert!(matches!(parser.parse(b"\x1b]52;;SGVsbG8=\x07").as_slice(), [Event::Clipboard(t)] if t == "Hello"));
        assert!(matches!(parser.parse(b"\x1b]52;c;\x07").as_slice(), [Event::Clipboard(t)] if t.is_empty()));
    }
    #[test]
    fn malformed_unknown_and_oversized_osc_do_not_leak_keys() {
        let mut parser = Parser::new();
        for packet in [b"\x1b]52;c;?\x07".as_slice(), b"\x1b]52;c;SGVsbG8!\x07",
                       b"\x1b]52;c;AB==\x07", b"\x1b]52;c;/w==\x07", b"\x1b]0;ignored\x07"] {
            assert!(parser.parse(packet).is_empty());
        }
        parser.parse(b"\x1b]52;c;");
        assert!(parser.parse(&vec![b'A'; 24 * 1024 * 1024 + 1]).is_empty());
        assert!(parser.parse(b"\x03\x1b[A\x1b\\").is_empty());
        assert!(matches!(parser.parse(b"x").as_slice(), [Event::KeyPress { .. }]));
    }
    #[test]
    fn oversized_paste_is_discarded_until_its_end() {
        let mut parser = Parser::new();
        parser.parse(b"\x1b[200~");
        assert!(parser.parse(&vec![b'x'; 16 * 1024 * 1024 + 1]).is_empty());
        assert!(parser.parse(b"\x03\x1b[A").is_empty());
        assert!(parser.parse(b"\x1b[201~").is_empty());
        assert!(matches!(parser.parse(b"a").as_slice(), [Event::KeyPress { .. }]));
    }
}
