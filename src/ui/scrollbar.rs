use crate::gfx::{Color, Point, Size};
use super::navigation::NavigationAction;

#[derive(Default)]
pub struct Scrollbar {
    offset: f32,
    viewport: f32,
    content: f32,
    drag: Option<u32>,
}

impl Scrollbar {
    pub fn update(&mut self, offset: f32, viewport: f32, content: f32) {
        if [offset, viewport, content].iter().all(|v| v.is_finite()) {
            self.viewport = viewport.max(0.0);
            self.content = content.max(self.viewport);
            self.offset = offset.clamp(0.0, self.maximum());
        }
    }
    fn maximum(&self) -> f32 { (self.content - self.viewport).max(0.0) }
    pub fn thumb(&self, rows: u32) -> Option<(u32, u32)> {
        if rows == 0 || self.viewport <= 0.0 || self.maximum() <= 0.5 { return None; }
        let height = ((rows as f32 * self.viewport / self.content).round() as u32).clamp(1, rows);
        let top = ((rows - height) as f32 * self.offset / self.maximum()).round() as u32;
        Some((top, height))
    }
    pub fn color(&self, row: u32, rows: u32) -> Option<Color> {
        self.thumb(rows).map(|(top, height)| if row >= top && row < top + height {
            Color::splat(192)
        } else { Color::splat(48) })
    }
    fn target(&self, top: u32, rows: u32) -> NavigationAction {
        let (_, height) = self.thumb(rows).unwrap();
        let travel = rows - height;
        let offset = if travel == 0 { 0.0 } else { top.min(travel) as f32 / travel as f32 * self.maximum() };
        NavigationAction::ScrollTo(offset)
    }
    pub fn down(&mut self, point: Point, size: Size) -> Option<NavigationAction> {
        if point.y < 1 || point.x != size.width.saturating_sub(1) as i32 { return None; }
        let Some((top, height)) = self.thumb(size.height) else { return Some(NavigationAction::Ignore); };
        let row = (point.y as u32 - 1).min(size.height.saturating_sub(1));
        let grab = if row >= top && row < top + height { row - top } else { height / 2 };
        self.drag = Some(grab);
        Some(self.target(row.saturating_sub(grab), size.height))
    }
    pub fn motion(&mut self, point: Point, size: Size) -> Option<NavigationAction> {
        let grab = self.drag?;
        if self.thumb(size.height).is_none() { self.drag = None; return Some(NavigationAction::Ignore); }
        Some(self.target((point.y.max(1) as u32 - 1).saturating_sub(grab), size.height))
    }
    pub fn up(&mut self, point: Point, size: Size) -> Option<NavigationAction> {
        let action = self.motion(point, size);
        self.drag = None;
        action.or_else(|| if point.y >= 1 && point.x == size.width.saturating_sub(1) as i32 { Some(NavigationAction::Ignore) } else { None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thumb_tracks_viewport_and_clamps_at_both_ends() {
        let mut bar = Scrollbar::default();
        bar.update(0.0, 100.0, 1000.0);
        assert_eq!(bar.thumb(30), Some((0, 3)));
        bar.update(900.0, 100.0, 1000.0);
        assert_eq!(bar.thumb(30), Some((27, 3)));
        bar.update(5000.0, 100.0, 1000.0);
        assert_eq!(bar.thumb(30), Some((27, 3)));
        bar.update(0.0, 100.0, 100.0);
        assert_eq!(bar.thumb(30), None);
    }
    #[test]
    fn drag_can_reach_bottom_and_release_outside_the_column() {
        let mut bar = Scrollbar::default();
        bar.update(0.0, 100.0, 1000.0);
        let size = Size::new(80, 30);
        bar.down(Point::new(79, 1), size);
        assert!(matches!(bar.motion(Point::new(40, 30), size), Some(NavigationAction::ScrollTo(v)) if v == 900.0));
        bar.up(Point::new(40, 30), size);
        assert!(bar.motion(Point::new(40, 1), size).is_none());
    }
}
