use crate::gfx::{Color, Point, Size};
use super::Cell;

pub struct Selection {
    anchor: Option<Point>,
    end: Point,
    pressed: bool,
    dragged: bool,
}
impl Default for Selection {
    fn default() -> Self {
        Self { anchor: None, end: Point::new(0, 0), pressed: false, dragged: false }
    }
}

impl Selection {
    pub fn clear(&mut self) { *self = Self::default(); }
    pub fn active(&self) -> bool { self.anchor.is_some() && self.dragged }
    pub fn pressed(&self) -> bool { self.pressed }
    fn clamp(point: Point, size: Size) -> Point {
        Point::new(point.x.clamp(0, size.width.saturating_sub(2) as i32),
            point.y.clamp(1, size.height.max(1) as i32))
    }
    pub fn down(&mut self, point: Point, size: Size) {
        self.clear();
        self.anchor = Some(Self::clamp(point, size));
        self.end = self.anchor.unwrap();
        self.pressed = true;
    }
    pub fn motion(&mut self, point: Point, size: Size) {
        if self.pressed {
            self.end = Self::clamp(point, size);
            self.dragged |= self.anchor != Some(self.end);
        }
    }
    pub fn up(&mut self, point: Point, size: Size) -> Option<Point> {
        if !self.pressed { return None; }
        self.motion(point, size);
        self.pressed = false;
        if self.dragged { None } else {
            let point = self.anchor;
            self.clear();
            point
        }
    }
    fn range(&self, size: Size) -> Option<(usize, usize)> {
        if !self.active() || size.width < 2 || size.height == 0 { return None; }
        let offset = |point: Point| point.y as usize * size.width as usize + point.x as usize;
        let start = offset(self.anchor?);
        let end = offset(self.end);
        Some((start.min(end), start.max(end)))
    }
    pub fn text(&self, cells: &[(Cell, Cell)], size: Size) -> String {
        let Some((start, end)) = self.range(size) else { return String::new(); };
        let width = size.width as usize;
        let mut lines = Vec::new();
        for row in start / width ..= end / width {
            let mut column = if row == start / width { start % width } else { 0 };
            let last = if row == end / width { end % width } else { width - 2 };
            let mut text = String::new();
            while column <= last {
                let Some((_, cell)) = cells.get(row * width + column) else { break; };
                if let Some(grapheme) = &cell.grapheme {
                    text.push_str(&grapheme.char);
                    column += grapheme.width.saturating_sub(grapheme.index).max(1);
                } else {
                    // Raster/Chafa symbols are decoration, never copied as text.
                    text.push(' ');
                    column += 1;
                }
            }
            lines.push(text.trim_end().to_owned());
        }
        lines.join("\n")
    }
    pub fn highlight(&self, cells: &mut [(Cell, Cell)], size: Size) {
        let Some((start, end)) = self.range(size) else { return; };
        let width = size.width as usize;
        let color = Color::new(35, 70, 115);
        let paint = |cell: &mut Cell| {
            cell.quadrant = (color, color, color, color);
            cell.graphic.background = color;
            cell.graphic.foreground = cell.graphic.foreground.avg_with(color);
            if let Some(grapheme) = &cell.grapheme {
                let mut grapheme = (**grapheme).clone();
                grapheme.color = Color::splat(255);
                cell.grapheme = Some(std::rc::Rc::new(grapheme));
            }
        };
        for index in start ..= end.min(cells.len().saturating_sub(1)) {
            if index % width >= width - 1 { continue; }
            // Selecting either cell of a wide Unicode grapheme highlights the
            // whole character, including its lead cell where Painter emits it.
            let (first, count) = cells[index].1.grapheme.as_ref()
                .map_or((index, 1), |g| (index.saturating_sub(g.index), g.width));
            for cell in &mut cells[first .. (first + count).min(index / width * width + width - 1)] {
                paint(&mut cell.1);
            }
        }
    }
}

pub fn encode_clipboard(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let word = (chunk[0] as u32) << 16 | (chunk.get(1).copied().unwrap_or(0) as u32) << 8 |
            chunk.get(2).copied().unwrap_or(0) as u32;
        result.push(ALPHABET[(word >> 18) as usize] as char);
        result.push(ALPHABET[(word >> 12 & 63) as usize] as char);
        result.push(if chunk.len() > 1 { ALPHABET[(word >> 6 & 63) as usize] as char } else { '=' });
        result.push(if chunk.len() > 2 { ALPHABET[(word & 63) as usize] as char } else { '=' });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Renderer;
    #[test]
    fn selection_copies_only_text_in_both_directions_and_handles_wide_graphemes() {
        let size = Size::new(12, 3);
        let mut renderer = Renderer::new();
        renderer.set_size(size);
        renderer.draw_text("żółć中", Point::new(2, 8), Size::splat(0), Color::splat(200));
        renderer.draw_text("e\u{301} END", Point::new(0, 12), Size::splat(0), Color::splat(200));
        renderer.mouse_down(Point::new(1, 2)).unwrap();
        renderer.mouse_move(Point::new(4, 3)).unwrap();
        renderer.mouse_up(Point::new(4, 3)).unwrap();
        assert_eq!(renderer.selected_text(), "żółć中\ne\u{301} END");
        renderer.mouse_down(Point::new(4, 3)).unwrap();
        renderer.mouse_move(Point::new(1, 2)).unwrap();
        renderer.mouse_up(Point::new(1, 2)).unwrap();
        assert_eq!(renderer.selected_text(), "żółć中\ne\u{301} END");
        renderer.mouse_down(Point::new(6, 2)).unwrap();
        renderer.mouse_move(Point::new(5, 2)).unwrap();
        renderer.mouse_up(Point::new(5, 2)).unwrap();
        assert_eq!(renderer.selected_text(), "中");
        assert_eq!(encode_clipboard("żółć".as_bytes()), "xbzDs8WCxIc=");
        assert_eq!(encode_clipboard(b"f"), "Zg==");
        assert_eq!(encode_clipboard(b"foo"), "Zm9v");
        assert_eq!(encode_clipboard(b""), "");
    }
}
