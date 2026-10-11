use std::ffi::c_void;
use std::ptr::NonNull;

use crate::gfx::{Color, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graphic {
    pub character: char,
    pub foreground: Color,
    pub background: Color,
}

impl Graphic {
    pub fn solid(color: Color) -> Self {
        Self { character: ' ', foreground: color, background: color }
    }
}

extern "C" {
    fn carbonyl_chafa_new(width: i32, height: i32) -> *mut c_void;
    fn carbonyl_chafa_free(canvas: *mut c_void);
    fn carbonyl_chafa_draw(canvas: *mut c_void, pixels: *const u8,
                          width: i32, height: i32, stride: i32);
    fn carbonyl_chafa_get(canvas: *mut c_void, x: i32, y: i32,
                         foreground: *mut i32, background: *mut i32) -> u32;
}

/// Owned by the existing rendering thread; never shared between threads.
pub struct Chafa {
    canvas: Option<NonNull<c_void>>,
    size: Size,
}

impl Chafa {
    pub fn new() -> Self {
        Self { canvas: None, size: Size::new(0, 0) }
    }

    pub fn draw(&mut self, pixels: &[u8], pixels_size: Size, cells: Size) -> bool {
        let width = cells.width as usize * 2;
        let height = cells.height as usize * 4;
        let stride = pixels_size.width as usize * 4;
        if cells.width == 0 || cells.height == 0 ||
            width > pixels_size.width as usize || height > pixels_size.height as usize ||
            stride > i32::MAX as usize || pixels.len() < stride * pixels_size.height as usize {
            return false;
        }
        if self.size != cells {
            if let Some(canvas) = self.canvas.take() {
                unsafe { carbonyl_chafa_free(canvas.as_ptr()) };
            }
            self.canvas = NonNull::new(unsafe {
                carbonyl_chafa_new(cells.width as i32, cells.height as i32)
            });
            self.size = cells;
        }
        let Some(canvas) = self.canvas else { return false };
        unsafe {
            carbonyl_chafa_draw(canvas.as_ptr(), pixels.as_ptr(),
                               width as i32, height as i32, stride as i32);
        }
        true
    }

    pub fn get(&self, x: usize, y: usize, fallback: Color) -> Graphic {
        let (mut foreground, mut background) = (-1, -1);
        let character = unsafe {
            carbonyl_chafa_get(self.canvas.unwrap().as_ptr(), x as i32, y as i32,
                              &mut foreground, &mut background)
        };
        let color = |rgb: i32| {
            if rgb < 0 { fallback } else {
                Color::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
            }
        };
        Graphic {
            character: char::from_u32(character).filter(|c| *c != '\0').unwrap_or(' '),
            foreground: color(foreground),
            background: color(background),
        }
    }
}

impl Drop for Chafa {
    fn drop(&mut self) {
        if let Some(canvas) = self.canvas {
            unsafe { carbonyl_chafa_free(canvas.as_ptr()) };
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_pattern_keeps_different_foreground_and_background() {
        let mut chafa = Chafa::new();
        let mut pixels = Vec::new();
        for y in 0..4 {
            for _ in 0..2 {
                let v = if y < 2 { 255 } else { 0 };
                pixels.extend_from_slice(&[v, v, v, 255]);
            }
        }
        assert!(chafa.draw(&pixels, Size::new(2, 4), Size::new(1, 1)));
        let graphic = chafa.get(0, 0, Color::splat(127));
        assert_ne!(graphic.foreground, graphic.background,
            "Opaque Chafa pixels must not collapse to the fallback cell color");
        assert!(matches!(graphic.character, '\u{2580}' | '\u{2584}' | '\u{258c}' | '\u{2590}' |
            '\u{2596}'..='\u{259f}' | '\u{1fb00}'..='\u{1fb3b}'));
    }
}
