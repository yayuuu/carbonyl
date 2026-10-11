use std::rc::Rc;

use crate::gfx::{Color, Point};
use super::Graphic;

#[derive(Clone, PartialEq)]
pub struct Grapheme {
    /// Unicode character in UTF-8, might contain multiple code points (Emoji, CJK).
    pub char: String,
    pub index: usize,
    pub width: usize,
    pub color: Color,
}

/// Terminal cell with `height = width * 2`
#[derive(Clone, PartialEq)]
pub struct Cell {
    pub cursor: Point<u32>,
    /// Text grapheme if any
    pub grapheme: Option<Rc<Grapheme>>,
    pub quadrant: (Color, Color, Color, Color),
    pub graphic: Graphic,
}

impl Cell {
    pub fn new(x: u32, y: u32) -> Cell {
        Cell {
            cursor: Point::new(x, y),
            grapheme: None,
            graphic: Graphic::solid(Color::black()),
            quadrant: (
                Color::black(),
                Color::black(),
                Color::black(),
                Color::black(),
            ),
        }
    }
}
