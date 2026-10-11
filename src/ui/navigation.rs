use std::env;

use unicode_width::UnicodeWidthStr;

use crate::{
    gfx::{Color, Point, Size},
    input::Key,
    utils::log,
};

pub enum NavigationAction {
    Ignore,
    DialogAction(u64, i32, i32, String),
    ScrollTo(f32),
    Paste(String),
    Click(Point),
    Forward,
    GoTo(String),
    GoBack(),
    GoForward(),
    Refresh(),
}

#[derive(Debug)]
pub struct NavigationElement {
    pub text: String,
    pub background: Color,
    pub foreground: Color,
}

pub struct Navigation {
    url: Option<String>,
    size: Size,
    cursor: Option<usize>,
    select_all: bool,
    can_go_back: bool,
    can_go_forward: bool,
}

impl Navigation {
    pub fn new() -> Self {
        Self {
            url: None,
            size: (0, 0).into(),
            cursor: None,
            select_all: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    pub fn cursor(&self) -> Option<Point> {
        let cursor = self.cursor?;
        let column = 11 + self.display_url()[..cursor].width() as i32;
        Some((column.min((self.size.width as i32 - 2).max(0)), 0).into())
    }

    pub fn paste(&mut self,text:&str)->NavigationAction {
        if let (Some(cursor),Some(url))=(self.cursor,&mut self.url) {
            let cursor=if self.select_all {url.clear();0} else {cursor};
            let text=text.chars().filter(|c|!c.is_control()).collect::<String>();
            url.insert_str(cursor,&text);self.cursor=Some(cursor+text.len());self.select_all=false;
            NavigationAction::Ignore
        } else {NavigationAction::Paste(text.into())}
    }
    pub fn keypress(&mut self, key: &Key) -> NavigationAction {
        let modifier_key = match env::consts::OS {
            "macos" => key.modifiers.meta,
            _ => key.modifiers.alt,
        };

        match self.cursor {
            None => match (modifier_key, key.char) {
                (true, 0x14) => NavigationAction::GoBack(),
                (true, 0x13) => NavigationAction::GoForward(),
                _ => NavigationAction::Forward,
            },
            Some(cursor) => {
                if let Some(url) = &mut self.url {
                    match key.char {
                        // Return
                        0x0d => {
                            self.cursor = None;
                            self.select_all = false;
                            return NavigationAction::GoTo(url.clone());
                        }
                        // Escape leaves the address field.
                        0x1b => {
                            self.cursor = None;
                            self.select_all = false;
                        }
                        // Select all (Ctrl+A).
                        0x01 => {
                            self.select_all = true;
                            self.cursor = Some(0);
                        }
                        // Up
                        0x11 => {
                            self.select_all = false;
                            self.cursor = Some(0);
                        }
                        // Down
                        0x12 => {
                            self.select_all = false;
                            self.cursor = Some(url.len());
                        }
                        // Right
                        0x13 => {
                            self.cursor = Some(if self.select_all {
                                url.len()
                            } else {
                                cursor + url[cursor..].chars().next().map_or(0, char::len_utf8)
                            });
                            self.select_all = false;
                        }
                        // Left
                        0x14 => {
                            self.cursor = Some(if self.select_all {
                                0
                            } else {
                                cursor - url[..cursor].chars().next_back().map_or(0, char::len_utf8)
                            });
                            self.select_all = false;
                        }
                        // Backspace
                        0x08 | 0x7f => {
                            if self.select_all {
                                url.clear();
                                self.select_all = false;
                                self.cursor = Some(0);
                            } else if cursor > 0 {
                                let previous = cursor - url[..cursor].chars().next_back().unwrap().len_utf8();
                                url.remove(previous);
                                self.cursor = Some(previous);
                            }
                        }
                        key if key >= 0x20 => {
                            let cursor = if self.select_all {
                                url.clear();
                                self.select_all = false;
                                0
                            } else {
                                cursor
                            };
                            let character = key as char;
                            url.insert(cursor, character);
                            self.cursor = Some(cursor + character.len_utf8());
                        }
                        _ => {}
                    }

                    NavigationAction::Ignore
                } else {
                    NavigationAction::Forward
                }
            }
        }
    }

    pub fn display_url(&self) -> &str {
        match &self.url {
            None => "about:blank",
            Some(url) => url,
        }
    }

    pub fn url_size(&self) -> usize {
        self.display_url().width()
    }

    pub fn mouse_up(&mut self, origin: Point) -> NavigationAction {
        if origin.y != 0 {
            self.cursor = None;
            self.select_all = false;

            NavigationAction::Forward
        } else {
            NavigationAction::Ignore
        }
    }
    pub fn mouse_down(&mut self, origin: Point) -> NavigationAction {
        if origin.y != 0 {
            self.cursor = None;
            self.select_all = false;

            return NavigationAction::Forward;
        }

        self.cursor = None;
        self.select_all = false;

        return match origin.x {
            0..=2 => NavigationAction::GoBack(),
            3..=5 => NavigationAction::GoForward(),
            6..=8 => NavigationAction::Refresh(),
            9.. => {
                self.url.get_or_insert_with(|| "about:blank".to_owned());
                self.cursor = Some(0);
                self.select_all = true;

                log::debug!("setting cursor to {:?}", self.cursor);

                NavigationAction::Ignore
            }
            _ => NavigationAction::Ignore,
        };
    }
    pub fn mouse_move(&mut self, _origin: Point) -> NavigationAction {
        NavigationAction::Forward
    }

    pub fn push(&mut self, url: &str, can_go_back: bool, can_go_forward: bool) {
        if match (self.cursor, &self.url) {
            (None, _) => false,
            (_, None) => true,
            (_, Some(current)) => current != url,
        } {
            self.cursor = Some(url.len());
            self.select_all = false;
        }

        self.url = Some(url.to_owned());
        self.can_go_back = can_go_back;
        self.can_go_forward = can_go_forward;
    }

    pub fn set_size(&mut self, size: Size) {
        self.size = size
    }

    pub fn render_btn(&self, icon: &str, enabled: bool) -> [NavigationElement; 3] {
        let background = Color::splat(255);
        let foreground = Color::splat(0);

        [
            NavigationElement {
                text: "[".to_owned(),
                background,
                foreground,
            },
            NavigationElement {
                text: icon.to_owned(),
                background,
                foreground: if enabled {
                    foreground
                } else {
                    Color::splat(200)
                },
            },
            NavigationElement {
                text: "]".to_owned(),
                background,
                foreground,
            },
        ]
    }

    pub fn render(&self, size: Size) -> Vec<(Point, NavigationElement)> {
        let ui_elements = 13;
        let space = if size.width >= ui_elements {
            (size.width - ui_elements) as usize
        } else {
            0
        };
        let url: String = self.display_url().chars().take(space).collect();
        let width = url.width();
        let padded = format!(" {}{} ", url, " ".repeat(space - width));
        let mut elements = Vec::new();
        let mut point = Point::splat(0);

        let mut address = self.render_btn(&padded, true);
        if self.select_all {
            address[1].background = Color::new(40, 80, 160);
            address[1].foreground = Color::splat(255);
        }

        for list in [
            self.render_btn("\u{276e}", self.can_go_back),
            self.render_btn("\u{276f}", self.can_go_forward),
            self.render_btn("↻", true),
            address,
        ] {
            for element in list {
                let width = element.text.width() as i32;

                elements.push((point.clone(), element));

                point = point + (width, 0);
            }
        }

        elements
    }
}
