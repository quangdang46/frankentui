#![forbid(unsafe_code)]

//! Masonry grid widget: auto-computes column count from available width and ideal
//! tile width, placing items left-to-right, top-to-bottom with varying heights.
//!
//! # Layout Algorithm
//!
//! 1. **Column count** is computed as `max(1, available_width / ideal_width)`.
//!    The available width is divided equally among columns (minus gaps).
//! 2. **Tile placement** follows a shortest-column strategy: the next tile is placed
//!    in the column with the least cumulative height, producing a natural masonry
//!    packing.
//! 3. **Overflow strip** (`+N more`) is rendered when not all items fit vertically
//!    within the available height.
//! 4. **Clickable tiles** register hit regions when a `hit_id` is provided.
//! 5. **Degradation-aware**: at reduced budgets, decorative elements are skipped.

use crate::{Widget, clear_text_area};
use ftui_core::geometry::Rect;
use ftui_render::budget::DegradationLevel;
use ftui_render::frame::{Frame, HitId, HitRegion};
use ftui_style::Style;

/// A masonry grid that auto-computes its column layout.
#[derive(Debug)]
pub struct MasonryGrid<'a> {
    tiles: Vec<MasonryTile<'a>>,
    ideal_width: u16,
    gap: u16,
    hit_id: Option<HitId>,
    overflow_style: Style,
    tile_style: Style,
}

impl Default for MasonryGrid<'_> {
    fn default() -> Self {
        Self {
            tiles: Vec::new(),
            ideal_width: 20,
            gap: 1,
            hit_id: None,
            overflow_style: Style::default().dim(),
            tile_style: Style::default(),
        }
    }
}

impl<'a> MasonryGrid<'a> {
    #[must_use]
    pub fn new() -> Self { Self::default() }

    #[must_use]
    pub fn ideal_width(mut self, width: u16) -> Self { self.ideal_width = width.max(1); self }

    #[must_use]
    pub fn gap(mut self, gap: u16) -> Self { self.gap = gap; self }

    #[must_use]
    pub fn tile(mut self, tile: MasonryTile<'a>) -> Self { self.tiles.push(tile); self }

    #[must_use]
    pub fn hit_id(mut self, id: HitId) -> Self { self.hit_id = Some(id); self }

    #[must_use]
    pub fn overflow_style(mut self, style: Style) -> Self { self.overflow_style = style; self }

    #[must_use]
    pub fn tile_style(mut self, style: Style) -> Self { self.tile_style = style; self }

    fn column_count(&self, available_width: u16) -> u16 {
        if self.ideal_width == 0 || available_width == 0 { return 1; }
        let count = available_width / self.ideal_width;
        let capped = if self.tiles.is_empty() { count } else { count.min(self.tiles.len() as u16) };
        capped.max(1)
    }

    fn layout_columns(&self, total_width: u16) -> (u16, u16) {
        let count = self.column_count(total_width);
        if count == 1 { return (total_width, 1); }
        let total_gaps = self.gap.saturating_mul(count.saturating_sub(1));
        let col_width = if total_gaps >= total_width { 1 } else { (total_width - total_gaps) / count };
        (col_width, count)
    }

    fn place_tiles(&self, col_width: u16, col_count: u16, area: Rect) -> (Vec<(usize, u16, Rect)>, usize) {
        if col_count == 0 || col_width == 0 || self.tiles.is_empty() { return (Vec::new(), 0); }
        let mut col_heights: Vec<u16> = vec![0; col_count as usize];
        let mut placements: Vec<(usize, u16, Rect)> = Vec::new();
        let mut overflow_count: usize = 0;
        let start_x = area.x;
        let start_y = area.y;
        let max_bottom = area.bottom();
        for (tile_idx, tile) in self.tiles.iter().enumerate() {
            let (shortest_col, &shortest_height) = col_heights.iter().enumerate().min_by_key(|&(_, h)| h).unwrap();
            let tile_x = start_x + (shortest_col as u16) * (col_width + self.gap);
            let tile_y = start_y + shortest_height;
            let tile_height = tile.height.max(1);
            if tile_y + tile_height > max_bottom {
                overflow_count = self.tiles.len() - tile_idx;
                break;
            }
            let tile_rect = Rect::new(tile_x, tile_y, col_width, tile_height);
            placements.push((shortest_col, tile_idx as u16, tile_rect));
            col_heights[shortest_col] = shortest_height + tile_height + self.gap;
        }
        (placements, overflow_count)
    }
}

impl Widget for MasonryGrid<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() || self.tiles.is_empty() { return; }
        let level = frame.degradation;
        let render_content = level <= DegradationLevel::EssentialOnly;
        let render_overflow = level <= DegradationLevel::Skeleton;
        let (col_width, col_count) = self.layout_columns(area.width);
        if col_width == 0 || col_count == 0 { return; }
        let (placements, overflow_count) = self.place_tiles(col_width, col_count, area);
        for (_col_idx, tile_idx, tile_rect) in &placements {
            let tile = &self.tiles[*tile_idx as usize];
            clear_text_area(frame, *tile_rect, self.tile_style);
            if render_content {
                let mut guard = ScissorGuard::new(frame, *tile_rect);
                tile.widget.render(*tile_rect, guard.frame_mut());
            }
            if let Some(hit_id) = self.hit_id {
                frame.register_hit(*tile_rect, hit_id, HitRegion::Content, *tile_idx as u64);
            }
        }
        if render_overflow && overflow_count > 0 {
            let overflow_text = format!("+{overflow_count} more");
            let text_width = ftui_text::display_width(&overflow_text) as u16;
            let overflow_y = area.bottom().saturating_sub(1);
            let draw_x = area.right().saturating_sub(text_width).max(area.x);
            let mut guard = ScissorGuard::new(frame, area);
            let f = guard.frame_mut();
            let _ = crate::draw_text_span(f, draw_x, overflow_y, &overflow_text, self.overflow_style, area.right());
        }
    }

    fn is_essential(&self) -> bool { !self.tiles.is_empty() }
}

pub struct MasonryTile<'a> {
    widget: Box<dyn Widget + 'a>,
    height: u16,
}

impl std::fmt::Debug for MasonryTile<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MasonryTile").field("widget", &"<dyn Widget>").field("height", &self.height).finish()
    }
}

impl<'a> MasonryTile<'a> {
    #[must_use]
    pub fn new(widget: impl Widget + 'a, height: u16) -> Self { Self { widget: Box::new(widget), height: height.max(1) } }
    #[must_use]
    pub fn height(mut self, height: u16) -> Self { self.height = height.max(1); self }
}

struct ScissorGuard<'a, 'pool> {
    frame: &'a mut Frame<'pool>,
}

impl<'a, 'pool> ScissorGuard<'a, 'pool> {
    fn new(frame: &'a mut Frame<'pool>, rect: Rect) -> Self { frame.buffer.push_scissor(rect); Self { frame } }
    fn frame_mut(&mut self) -> &mut Frame<'pool> { self.frame }
}

impl Drop for ScissorGuard<'_, '_> {
    fn drop(&mut self) { self.frame.buffer.pop_scissor(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_core::geometry::Rect;
    use ftui_render::budget::DegradationLevel;
    use ftui_render::cell::Cell;
    use ftui_render::grapheme_pool::GraphemePool;
    use ftui_render::frame::HitId;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug)]
    struct RecordWidget { rects: Rc<RefCell<Vec<Rect>>> }
    impl RecordWidget {
        fn new() -> (Self, Rc<RefCell<Vec<Rect>>>) {
            let rects = Rc::new(RefCell::new(Vec::new())); (Self { rects: rects.clone() }, rects)
        }
    }
    impl Widget for RecordWidget {
        fn render(&self, area: Rect, _frame: &mut Frame) { self.rects.borrow_mut().push(area); }
    }

    #[derive(Clone, Debug)]
    struct Marker { ch: char }
    impl Widget for Marker {
        fn render(&self, area: Rect, frame: &mut Frame) { frame.buffer.set(area.x, area.y, Cell::from_char(self.ch)); }
    }

    #[test] fn column_count_20() { let g = MasonryGrid::new().ideal_width(20).tile(MasonryTile::new(Marker{ch:'A'},3)).tile(MasonryTile::new(Marker{ch:'B'},3)).tile(MasonryTile::new(Marker{ch:'C'},3)).tile(MasonryTile::new(Marker{ch:'D'},3)).tile(MasonryTile::new(Marker{ch:'E'},3)); assert_eq!(g.column_count(80), 4); }
    #[test] fn column_count_min_1() { let g = MasonryGrid::new().ideal_width(100); assert_eq!(g.column_count(50), 1); }
    #[test] fn column_count_zero() { let g = MasonryGrid::new().ideal_width(20).tile(MasonryTile::new(Marker{ch:'A'},3)); assert_eq!(g.column_count(0), 1); }
    #[test] fn column_count_clamped() { let g = MasonryGrid::new().ideal_width(10).tile(MasonryTile::new(Marker{ch:'A'},3)).tile(MasonryTile::new(Marker{ch:'B'},3)); assert_eq!(g.column_count(80), 2); }
    #[test] fn layout_single() { let (w,c) = MasonryGrid::new().ideal_width(100).tile(MasonryTile::new(Marker{ch:'A'},3)).layout_columns(50); assert_eq!(c,1); assert_eq!(w,50); }
    #[test] fn layout_multi_gap() { let (w,c) = MasonryGrid::new().ideal_width(20).gap(1).tile(MasonryTile::new(Marker{ch:'A'},3)).tile(MasonryTile::new(Marker{ch:'B'},3)).tile(MasonryTile::new(Marker{ch:'C'},3)).layout_columns(43); assert_eq!(c,2); assert_eq!(w,21); }
    #[test] fn layout_wide() { let (w,c) = MasonryGrid::new().ideal_width(20).gap(0).tile(MasonryTile::new(Marker{ch:'A'},3)).tile(MasonryTile::new(Marker{ch:'B'},3)).tile(MasonryTile::new(Marker{ch:'C'},3)).tile(MasonryTile::new(Marker{ch:'D'},3)).tile(MasonryTile::new(Marker{ch:'E'},3)).layout_columns(100); assert_eq!(c,5); assert_eq!(w,20); }
    #[test] fn place_lr() { let p = MasonryGrid::new().ideal_width(15).gap(0).tile(MasonryTile::new(Marker{ch:'A'},3)).tile(MasonryTile::new(Marker{ch:'B'},3)).place_tiles(15,2,Rect::new(0,0,30,10)); assert_eq!(p.0.len(),2); assert_eq!(p.0[0].2.x,0); assert_eq!(p.0[1].2.x,15); }
    #[test] fn place_shortest() { let p = MasonryGrid::new().ideal_width(15).gap(0).tile(MasonryTile::new(Marker{ch:'A'},5)).tile(MasonryTile::new(Marker{ch:'B'},2)).tile(MasonryTile::new(Marker{ch:'C'},2)).place_tiles(15,2,Rect::new(0,0,30,20)); assert_eq!(p.0.len(),3); assert_eq!(p.0[2].0,1); }
    #[test] fn place_overflow() { let p = MasonryGrid::new().ideal_width(20).gap(0).tile(MasonryTile::new(Marker{ch:'A'},3)).tile(MasonryTile::new(Marker{ch:'B'},3)).tile(MasonryTile::new(Marker{ch:'C'},3)).place_tiles(20,2,Rect::new(0,0,40,5)); assert_eq!(p.0.len(),2); assert_eq!(p.1,1); }
    #[test] fn place_all_fit() { let p = MasonryGrid::new().ideal_width(20).gap(1).tile(MasonryTile::new(Marker{ch:'A'},2)).tile(MasonryTile::new(Marker{ch:'B'},2)).tile(MasonryTile::new(Marker{ch:'C'},2)).place_tiles(20,2,Rect::new(0,0,41,6)); assert_eq!(p.1,0); assert_eq!(p.0.len(),3); }
    #[test] fn empty_no_placements() { let p = MasonryGrid::new().place_tiles(20,3,Rect::new(0,0,100,100)); assert!(p.0.is_empty()); assert_eq!(p.1,0); }
    #[test] fn render_empty() { let mut pool = GraphemePool::new(); let mut f = Frame::new(10,10,&mut pool); MasonryGrid::new().render(Rect::new(0,0,10,10),&mut f); }
    #[test] fn render_zero() { let mut pool = GraphemePool::new(); let mut f = Frame::new(5,5,&mut pool); MasonryGrid::new().tile(MasonryTile::new(Marker{ch:'A'},3)).render(Rect::new(0,0,0,0),&mut f); }
    #[test] fn render_tiles() { let (ra,rra)=RecordWidget::new(); let (rb,rrb)=RecordWidget::new(); let mut pool=GraphemePool::new(); let mut f=Frame::new(50,20,&mut pool); MasonryGrid::new().ideal_width(20).gap(0).tile(MasonryTile::new(ra,3)).tile(MasonryTile::new(rb,5)).render(Rect::new(2,2,40,10),&mut f); assert_eq!(rra.borrow().len(),1); assert_eq!(rrb.borrow().len(),1); }
    #[test] fn render_hit() { let mut pool=GraphemePool::new(); let mut f=Frame::with_hit_grid(40,10,&mut pool); MasonryGrid::new().ideal_width(20).gap(0).hit_id(HitId::new(99)).tile(MasonryTile::new(Marker{ch:'A'},3)).tile(MasonryTile::new(Marker{ch:'B'},3)).render(Rect::new(0,0,40,10),&mut f); assert!(f.hit_test(5,1).is_some()); assert_eq!(f.hit_test(25,1).unwrap().2,1); }
    #[test] fn deg_skip() { let (r,rr)=RecordWidget::new(); let mut pool=GraphemePool::new(); let mut f=Frame::new(40,10,&mut pool); f.degradation=DegradationLevel::Skeleton; MasonryGrid::new().tile(MasonryTile::new(r,3)).render(Rect::new(0,0,40,10),&mut f); assert!(rr.borrow().is_empty()); }
    #[test] fn deg_essential() { let (r,rr)=RecordWidget::new(); let mut pool=GraphemePool::new(); let mut f=Frame::new(40,10,&mut pool); f.degradation=DegradationLevel::EssentialOnly; MasonryGrid::new().tile(MasonryTile::new(r,3)).render(Rect::new(0,0,40,10),&mut f); assert_eq!(rr.borrow().len(),1); }
    #[test] fn essential_true() { assert!(MasonryGrid::new().tile(MasonryTile::new(Marker{ch:'A'},3)).is_essential()); }
    #[test] fn essential_false() { assert!(!MasonryGrid::new().is_essential()); }
    #[test] fn overflow_no_panic() { let mut pool=GraphemePool::new(); let mut f=Frame::new(40,12,&mut pool); MasonryGrid::new().ideal_width(20).gap(0).tile(MasonryTile::new(Marker{ch:'A'},10)).tile(MasonryTile::new(Marker{ch:'B'},10)).tile(MasonryTile::new(Marker{ch:'C'},10)).tile(MasonryTile::new(Marker{ch:'D'},10)).render(Rect::new(0,0,40,12),&mut f); }
    #[test] fn tile_min_height() { assert_eq!(MasonryTile::new(Marker{ch:'X'},0).height,1); }
    #[test] fn tile_height_setter() { assert_eq!(MasonryTile::new(Marker{ch:'X'},3).height(5).height,5); }
    #[test] fn defaults() { let g=MasonryGrid::new(); assert_eq!(g.ideal_width,20); assert_eq!(g.gap,1); assert!(g.tiles.is_empty()); assert!(g.hit_id.is_none()); }
    #[test] fn ideal_width_min() { assert_eq!(MasonryGrid::new().ideal_width(0).ideal_width,1); }
    #[test] fn gap_setter() { assert_eq!(MasonryGrid::new().gap(2).gap,2); }
    #[test] fn hit_id_setter() { assert_eq!(MasonryGrid::new().hit_id(HitId::new(77)).hit_id,Some(HitId::new(77))); }
    #[test] fn overflow_style_setter() { let g=MasonryGrid::new().overflow_style(Style::default().bold()); assert!(g.overflow_style.attrs.unwrap().contains(ftui_style::StyleFlags::BOLD)); }
    #[test] fn tile_style_setter() { let style=Style::default().bg(ftui_render::cell::PackedRgba::rgb(10,20,30)); let g=MasonryGrid::new().tile_style(style); assert_eq!(g.tile_style.bg,Some(ftui_render::cell::PackedRgba::rgb(10,20,30))); }
    #[test] fn many_fit() { let mut g=MasonryGrid::new().ideal_width(10).gap(0); for i in 0..50 { g=g.tile(MasonryTile::new(Marker{ch:char::from(b'A'+(i%26)as u8)},1)); } let (w,c)=g.layout_columns(100); let p=g.place_tiles(w,c,Rect::new(0,0,100,10)); assert_eq!(p.1,0); assert_eq!(p.0.len(),50); }
    #[test] fn small_overflow() { let mut pool=GraphemePool::new(); let mut f=Frame::new(30,3,&mut pool); MasonryGrid::new().ideal_width(50).gap(0).tile(MasonryTile::new(Marker{ch:'X'},5)).render(Rect::new(0,0,30,3),&mut f); }
    #[test] fn zero_width() { let mut pool=GraphemePool::new(); let mut f=Frame::new(1,10,&mut pool); MasonryGrid::new().ideal_width(20).tile(MasonryTile::new(Marker{ch:'A'},3)).render(Rect::new(0,0,0,10),&mut f); }
}
