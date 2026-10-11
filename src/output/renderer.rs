use std::{
    io::{self, Write},
    rc::Rc,
};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    gfx::{Color, Point, Rect, Size},
    input::Key,
    ui::navigation::{Navigation, NavigationAction},
    utils::log,
};

use super::{Cell, Chafa, Graphic, Grapheme, Painter};

pub struct Renderer {
    nav: Navigation,
    cells: Vec<(Cell, Cell)>,
    painter: Painter,
    size: Size,
    chafa: Chafa,
    scrollbar: crate::ui::scrollbar::Scrollbar,
    dialog: crate::ui::dialog::Dialog,
    download_status: String,
    clipboard_paste: Option<std::time::Instant>,
    selection: super::selection::Selection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_overlay_does_not_destroy_the_underlying_page() {
        let mut renderer=Renderer::new();renderer.set_size(Size::new(60,20));
        renderer.draw_text("PAGE UNDER DIALOG",Point::new(20,32),Size::splat(0),Color::splat(255));
        renderer.render().unwrap();
        let original=renderer.cells.iter().map(|(_,c)|c.clone()).collect::<Vec<_>>();
        renderer.update_dialog(1,"Question".into(),vec![crate::ui::dialog::Control{
            id:1,kind:1,label:"OK".into(),value:String::new(),flags:1}]);
        renderer.render().unwrap();
        assert!(renderer.cells.iter().zip(original).all(|((_,c),saved)|*c==saved));
    }

    #[test]
    fn graphics_do_not_replace_blinks_plain_text() {
        let mut renderer = Renderer::new();
        renderer.set_size(Size::new(24, 2));
        renderer.draw_text("Plain text", Point::new(0, 4), Size::splat(0), Color::splat(255));
        let mut pixels = Vec::new();
        for y in 0..8 {
            for x in 0..48 {
                let color = if x % 2 == 0 && y % 4 < 2 { 255 } else { 0 };
                pixels.extend_from_slice(&[color, color, color, 255]);
            }
        }
        renderer.draw_background(&pixels, Size::new(48, 8), Rect::new(0, 0, 48, 8));
        let text: String = renderer.cells[24..34].iter()
            .filter_map(|(_, c)| c.grapheme.as_ref().map(|g| g.char.clone())).collect();
        assert_eq!(text, "Plain text");
        assert!(renderer.cells[24..].iter().any(|(_, c)| c.graphic.character != ' '));
    }

    #[test]
    fn damage_updates_only_covered_cells_and_short_frames_are_ignored() {
        let mut renderer = Renderer::new();
        renderer.set_size(Size::new(3, 2));
        let pixels = [0, 0, 255, 255].repeat(6 * 8);
        renderer.draw_background(&pixels, Size::new(6, 8), Rect::new(2, 4, 2, 4));
        assert_eq!(renderer.cells[7].1.graphic.background, Color::new(255, 0, 0));
        assert_eq!(renderer.cells[6].1.graphic, Graphic::solid(Color::black()));
        renderer.draw_background(&pixels[..4], Size::new(6, 8), Rect::new(0, 0, 6, 8));
        assert_eq!(renderer.cells[7].1.graphic.background, Color::new(255, 0, 0));
    }

    #[test]
    fn terminal_drag_suppresses_page_drag_and_highlight_preserves_the_render() {
        let mut renderer = Renderer::new();
        renderer.set_size(Size::new(12, 3));
        renderer.draw_text("LOCAL TEXT", Point::new(0, 8), Size::splat(0), Color::splat(200));
        renderer.cells[24].1.graphic.character = '▚';
        renderer.render().unwrap();
        let original = renderer.cells.iter().map(|(_, cell)| cell.clone()).collect::<Vec<_>>();
        assert!(matches!(renderer.mouse_down(Point::new(0, 2)).unwrap(), NavigationAction::Ignore));
        assert!(matches!(renderer.mouse_move(Point::new(9, 2)).unwrap(), NavigationAction::Ignore));
        assert!(matches!(renderer.mouse_up(Point::new(9, 2)).unwrap(), NavigationAction::Ignore));
        assert_eq!(renderer.selected_text(), "LOCAL TEXT");
        renderer.render().unwrap();
        assert!(renderer.cells.iter().zip(original).all(|((_, cell), saved)| *cell == saved));
        assert!(matches!(renderer.keypress(&27.into()).unwrap(), NavigationAction::Ignore));
        assert_eq!(renderer.selected_text(), "");
        renderer.mouse_down(Point::new(4, 2)).unwrap();
        assert!(matches!(renderer.mouse_up(Point::new(4, 2)).unwrap(), NavigationAction::Click(_)));
    }
}

impl Renderer {
    pub fn new() -> Renderer { Self::new_fd(1) }
    pub fn new_fd(fd:i32) -> Renderer {
        Renderer {
            nav: Navigation::new(),
            cells: Vec::with_capacity(0),
            painter: Painter::new_fd(fd),
            size: Size::new(0, 0),
            chafa: Chafa::new(),
            scrollbar: Default::default(),
            dialog: Default::default(),
            download_status: String::new(),
            clipboard_paste: None,
            selection: Default::default(),
        }
    }

    pub fn enable_true_color(&mut self) {
        self.painter.set_true_color(true)
    }

    pub fn keypress(&mut self, key: &Key) -> io::Result<NavigationAction> {
        self.clipboard_paste = None;
        if key.char == 0x1b && self.selection.active() {
            self.selection.clear();
            return Ok(NavigationAction::Ignore);
        }
        self.selection.clear();
        if key.char == 0x16 && (self.dialog.id != 0 || self.nav.cursor().is_some()) {
            self.clipboard_paste = Some(std::time::Instant::now());
            self.query_clipboard()?;
            return Ok(NavigationAction::Ignore);
        }
        if self.dialog.id != 0 { return Ok(self.dialog.keypress(key)); }
        let action = self.nav.keypress(key);

        Ok(action)
    }
    pub fn mouse_up(&mut self, origin: Point) -> io::Result<NavigationAction> {
        // Always release a page drag, including one ending over the status bar.
        if self.dialog.id != 0 { return Ok(self.dialog.mouse(origin, self.size, true)); }
        if let Some(action) = self.scrollbar.up(origin, self.size) { return Ok(action); }
        if self.selection.pressed() {
            return Ok(self.selection.up(origin, self.size).map_or(NavigationAction::Ignore, NavigationAction::Click));
        }
        let action = self.nav.mouse_up(origin);

        Ok(action)
    }
    pub fn mouse_down(&mut self, origin: Point) -> io::Result<NavigationAction> {
        self.clipboard_paste = None;
        self.selection.clear();
        if !self.download_status.is_empty() && origin.y == self.size.height as i32 {return Ok(NavigationAction::Ignore);}
        if self.dialog.id != 0 { return Ok(self.dialog.mouse(origin, self.size, false)); }
        if let Some(action) = self.scrollbar.down(origin, self.size) {
            self.nav.mouse_down(origin);
            return Ok(action);
        }
        let action = self.nav.mouse_down(origin);
        if matches!(action, NavigationAction::Forward) && origin.y > 0 && self.size.width > 1 {
            self.selection.down(origin, self.size);
            return Ok(NavigationAction::Ignore);
        }
        Ok(action)
    }
    pub fn mouse_move(&mut self, origin: Point) -> io::Result<NavigationAction> {
        if !self.download_status.is_empty() && origin.y == self.size.height as i32 {return Ok(NavigationAction::Ignore);}
        if self.dialog.id != 0 { return Ok(NavigationAction::Ignore); }
        if let Some(action) = self.scrollbar.motion(origin, self.size) { return Ok(action); }
        if self.selection.pressed() {
            self.selection.motion(origin, self.size);
            return Ok(NavigationAction::Ignore);
        }
        if origin.y > 0 && origin.x == self.size.width.saturating_sub(1) as i32 { return Ok(NavigationAction::Ignore); }
        let action = self.nav.mouse_move(origin);

        Ok(action)
    }

    pub fn write_clipboard(&mut self, encoded:&str)->io::Result<()> {
        self.painter.escape(format!("\x1b]52;c;{encoded}\x07").as_bytes())
    }
    pub fn query_clipboard(&mut self) -> io::Result<()> {
        self.painter.escape(b"\x1b]52;c;?\x1b\\")
    }
    pub fn clipboard_reply(&mut self, text: &str) -> NavigationAction {
        if self.clipboard_paste.take().map_or(false, |start| start.elapsed().as_secs() < 2) {
            self.paste(text)
        } else { NavigationAction::Ignore }
    }
    pub fn paste(&mut self,text:&str)->NavigationAction {
        if self.dialog.id!=0 {
            let text=text.chars().map(|c|if c.is_control() {' '} else {c}).collect::<String>();
            let mut action=NavigationAction::Ignore;
            for byte in text.bytes() {action=self.dialog.keypress(&byte.into());}
            action
        } else {self.nav.paste(text)}
    }
    pub fn dialog_open(&self) -> bool { self.dialog.id != 0 }
    pub fn selected_text(&self) -> String { self.selection.text(&self.cells, self.size) }
    pub fn has_selection(&self) -> bool { self.selection.active() }
    pub fn copy_selection(&mut self) -> io::Result<Option<String>> {
        let text = self.selected_text();
        if text.is_empty() { return Ok(None); }
        self.write_clipboard(&super::selection::encode_clipboard(text.as_bytes()))?;
        Ok(Some(text))
    }
    pub fn update_dialog(&mut self, id:u64, title:String, controls:Vec<crate::ui::dialog::Control>) {
        if id != 0 { self.selection.clear(); }
        self.dialog.update(id,title,controls);
    }

    pub fn push_nav(&mut self, url: &str, can_go_back: bool, can_go_forward: bool) {
        if self.nav.display_url() != url { self.selection.clear(); }
        self.nav.push(url, can_go_back, can_go_forward)
    }

    pub fn update_scroll(&mut self, offset: f32, viewport: f32, content: f32) {
        self.scrollbar.update(offset, viewport, content);
    }

    pub fn set_download_status(&mut self, text: String) {
        self.download_status = text.chars().map(|c| if c.is_control() {' '} else {c}).collect();
    }

    pub fn get_size(&self) -> Size {
        self.size
    }

    pub fn set_size(&mut self, size: Size) {
        self.selection.clear();
        self.nav.set_size(size);
        self.size = size;

        let mut x = 0;
        let mut y = 0;
        let bound = size.width - 1;
        let cells = (size.width + size.width * size.height) as usize;

        self.cells.clear();
        self.cells.resize_with(cells, || {
            let cell = (Cell::new(x, y), Cell::new(x, y));

            if x < bound {
                x += 1;
            } else {
                x = 0;
                y += 1;
            }

            cell
        });
    }

    pub fn render(&mut self) -> io::Result<()> {
        let size = self.size;

        for (origin, element) in self.nav.render(size) {
            self.fill_rect(
                Rect::new(origin.x, origin.y, element.text.width() as u32, 1),
                element.background,
            );
            self.draw_text(
                &element.text,
                origin * (2, 1),
                Size::splat(0),
                element.foreground,
            );
        }

        let column = size.width.saturating_sub(1) as usize;
        for row in 0..size.height as usize {
            let cell = &mut self.cells[(row + 1) * size.width as usize + column].1;
            let color = self.scrollbar.color(row as u32, size.height);
            cell.grapheme = None;
            cell.graphic = Graphic { character: if color.is_some() { '█' } else { ' ' },
                foreground: color.unwrap_or(Color::black()), background: Color::black() };
            cell.quadrant = (Color::black(), Color::black(), Color::black(), Color::black());
        }
        // Keep the page buffers intact under the modal overlay, including new
        // frames arriving while a question is open. Closing restores the page.
        let underneath = if self.dialog.id != 0 || !self.download_status.is_empty() || self.selection.active() {
            Some(self.cells.iter().map(|(_, cell)| cell.clone()).collect::<Vec<_>>())
        } else { None };
        self.selection.highlight(&mut self.cells, size);
        if !self.download_status.is_empty() && size.width > 0 && size.height > 0 {
            let mut used=0usize;
            let text=UnicodeSegmentation::graphemes(self.download_status.as_str(),true)
                .take_while(|g| {used+=g.width(); used<=size.width as usize}).collect::<String>();
            self.fill_rect(Rect::new(0,size.height as i32,size.width,1),Color::new(25,45,65));
            self.draw_text(&text,Point::new(0,size.height as i32*4),Size::splat(0),Color::splat(240));
        }
        if self.dialog.id != 0 {
            for (point,text,focused) in self.dialog.render(size) {
                let bg=if focused {Color::new(35,70,115)} else {Color::new(25,28,34)};
                self.fill_rect(Rect::new(point.x,point.y,text.width() as u32,1),bg);
                self.draw_text(&text,point*(2,4),Size::splat(0),Color::splat(235));
            }
        }
        self.painter.begin()?;

        for (previous, current) in self.cells.iter_mut() {
            if current == previous {
                continue;
            }

            previous.quadrant = current.quadrant;
            previous.graphic = current.graphic;
            previous.grapheme = current.grapheme.clone();

            self.painter.paint(current)?;
        }

        self.painter.end(if self.dialog.id != 0 {None} else {self.nav.cursor()})?;
        if let Some(saved)=underneath {
            for ((_,cell),original) in self.cells.iter_mut().zip(saved) { *cell=original; }
        }

        Ok(())
    }

    /// Draw only the graphical background from Chromium's BGRA8888 surface.
    pub fn draw_background(&mut self, pixels: &[u8], pixels_size: Size, rect: Rect) {
        let viewport = self.size.cast::<usize>();

        if !self.chafa.draw(pixels, pixels_size, Size::new(self.size.width.saturating_sub(1).max(1), self.size.height)) {
            log::debug!(
                "unexpected size, actual: {}, expected: {}",
                pixels.len(),
                viewport.width * viewport.height * 8 * 4
            );
            return;
        }

        let origin = rect.origin.cast::<f32>().max(0.0) / (2.0, 4.0);
        let size = rect.size.cast::<f32>().max(0.0) / (2.0, 4.0);
        let top = (origin.y.floor() as usize).min(viewport.height);
        let left = (origin.x.floor() as usize).min(viewport.width);
        let right = ((origin.x + size.width).ceil() as usize)
            .min(viewport.width.saturating_sub(1))
            .max(left.min(viewport.width.saturating_sub(1)));
        let bottom = ((origin.y + size.height).ceil() as usize)
            .min(viewport.height)
            .max(top);
        let row_length = pixels_size.width as usize;
        let pixel = |x, y| {
            Color::new(
                pixels[((x + y * row_length) * 4 + 2) as usize],
                pixels[((x + y * row_length) * 4 + 1) as usize],
                pixels[((x + y * row_length) * 4 + 0) as usize],
            )
        };
        let pair = |x, y| pixel(x, y).avg_with(pixel(x, y + 1));

        for cell_y in top..bottom {
            let index = (cell_y + 1) * viewport.width;
            let start = index + left;
            let end = index + right;
            let (mut x, y) = (left * 2, cell_y * 4);

            for (_, cell) in &mut self.cells[start..end] {
                cell.quadrant = (
                    pair(x + 0, y + 0),
                    pair(x + 1, y + 0),
                    pair(x + 1, y + 2),
                    pair(x + 0, y + 2),
                );

                let (a, b, c, d) = cell.quadrant;
                let background = a.avg_with(b).avg_with(c.avg_with(d));
                cell.graphic = self.chafa.get(x / 2, cell_y, background);

                x += 2;
            }
        }
    }

    pub fn clear_text(&mut self) {
        for (_, cell) in self.cells.iter_mut() {
            cell.grapheme = None
        }
    }

    pub fn set_title(&mut self, title: &str) -> io::Result<()> {
        let mut stdout = Vec::new();

        write!(stdout, "\x1b]0;{title}\x07")?;
        write!(stdout, "\x1b]1;{title}\x07")?;
        write!(stdout, "\x1b]2;{title}\x07")?;

        self.painter.escape(&stdout)
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.draw(rect, |cell| {
            cell.grapheme = None;
            cell.quadrant = (color, color, color, color);
            cell.graphic = Graphic::solid(color);
        })
    }

    pub fn draw<F>(&mut self, bounds: Rect, mut draw: F)
    where
        F: FnMut(&mut Cell),
    {
        let origin = bounds.origin.cast::<usize>();
        let size = bounds.size.cast::<usize>();
        let viewport_width = self.size.width as usize;
        let top = origin.y;
        let bottom = top + size.height;

        // Iterate over each row
        for y in top..bottom {
            let left = y * viewport_width + origin.x;
            let right = left + size.width;

            for (_, current) in self.cells[left..right].iter_mut() {
                draw(current)
            }
        }
    }

    /// Render some text into the terminal output
    pub fn draw_text(&mut self, string: &str, origin: Point, size: Size, color: Color) {
        // Get an iterator starting at the text origin
        let len = self.cells.len();
        let viewport = &self.size.cast::<usize>();

        if size.width > 2 && size.height > 2 {
            let origin = (origin.cast::<f32>() / (2.0, 4.0) + (0.0, 1.0)).round();
            let size = (size.cast::<f32>() / (2.0, 4.0)).round();
            let left = (origin.x.max(0.0) as usize).min(viewport.width);
            let right = ((origin.x + size.width).max(0.0) as usize).min(viewport.width);
            let top = (origin.y.max(0.0) as usize).min(viewport.height);
            let bottom = ((origin.y + size.height).max(0.0) as usize).min(viewport.height);

            for y in top..bottom {
                let index = y * viewport.width;
                let start = index + left;
                let end = index + right;

                for (_, cell) in self.cells[start..end].iter_mut() {
                    cell.grapheme = None
                }
            }
        } else {
            // Compute the buffer index based on the position
            let index = origin.x / 2 + (origin.y + 1) / 4 * (viewport.width as i32);
            let mut iter = self.cells[len.min(index as usize)..].iter_mut();

            // Get every Unicode grapheme in the input string
            for grapheme in UnicodeSegmentation::graphemes(string, true) {
                let width = grapheme.width();

                for index in 0..width {
                    // Get the next terminal cell at the given position
                    match iter.next() {
                        // Stop if we're at the end of the buffer
                        None => return,
                        // Set the cell to the current grapheme
                        Some((_, cell)) => {
                            let next = Grapheme {
                                // Create a new shared reference to the text
                                color,
                                index,
                                width,
                                // Export the set of unicode code points for this graphene into an UTF-8 string
                                char: grapheme.to_string(),
                            };

                            if match cell.grapheme {
                                None => true,
                                Some(ref previous) => {
                                    previous.color != next.color || previous.char != next.char
                                }
                            } {
                                cell.grapheme = Some(Rc::new(next))
                            }
                        }
                    }
                }
            }
        }
    }
}
