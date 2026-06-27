#![forbid(unsafe_code)]
//! Text selection and copy mode for virtualized viewports.
use std::cmp::min;
use ftui_core::event::{KeyCode, KeyEvent, KeyEventKind};
use ftui_core::geometry::Rect;
use ftui_render::buffer::Buffer;
use ftui_style::Style;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode { #[default] Inactive, Normal }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelectionPosition { pub item: usize, pub col: u16 }
impl SelectionPosition {
    #[must_use] pub const fn new(item: usize, col: u16) -> Self { Self { item, col } }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange { pub start: SelectionPosition, pub end: SelectionPosition }
impl SelectionRange {
    #[must_use] pub fn normalized(a: SelectionPosition, b: SelectionPosition) -> Self {
        if a.item < b.item || (a.item == b.item && a.col <= b.col) { Self { start: a, end: b } }
        else { Self { start: b, end: a } }
    }
    #[must_use] pub fn contains(&self, pos: SelectionPosition) -> bool {
        if pos.item < self.start.item || pos.item > self.end.item { return false; }
        if pos.item == self.start.item && pos.col < self.start.col { return false; }
        if pos.item == self.end.item && pos.col > self.end.col { return false; }
        true
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionState {
    pub mode: SelectionMode, pub anchor: SelectionPosition, pub head: SelectionPosition, pub pending_copy: bool,
}
impl Default for SelectionState { fn default() -> Self { Self::new() } }
impl SelectionState {
    #[must_use] pub fn new() -> Self { Self { mode: SelectionMode::Inactive, anchor: SelectionPosition::default(), head: SelectionPosition::default(), pending_copy: false } }
    #[must_use] pub fn is_active(&self) -> bool { self.mode != SelectionMode::Inactive && self.anchor != self.head }
    #[must_use] pub fn normalized_range(&self) -> Option<SelectionRange> {
        if !self.is_active() { None } else { Some(SelectionRange::normalized(self.anchor, self.head)) }
    }
    pub fn enter_selection_mode(&mut self) { self.mode = SelectionMode::Normal; }
    pub fn exit_selection_mode(&mut self) { self.mode = SelectionMode::Inactive; self.head = self.anchor; }
    pub fn reset(&mut self) { *self = Self::new(); }
    pub fn move_up(&mut self) {
        if self.mode == SelectionMode::Inactive { if self.anchor.item > 0 { self.anchor.item -= 1; } self.head = self.anchor; }
        else if self.head.item > 0 { self.head.item -= 1; }
    }
    pub fn move_down(&mut self, ti: usize) {
        let m = ti.saturating_sub(1);
        if self.mode == SelectionMode::Inactive { if self.anchor.item < m { self.anchor.item += 1; } self.head = self.anchor; }
        else if self.head.item < m { self.head.item += 1; }
    }
    pub fn move_left(&mut self) {
        if self.mode == SelectionMode::Inactive { self.anchor.col = self.anchor.col.saturating_sub(1); self.head = self.anchor; }
        else { self.head.col = self.head.col.saturating_sub(1); }
    }
    pub fn move_right(&mut self, ll: u16) {
        if self.mode == SelectionMode::Inactive { self.anchor.col = min(self.anchor.col.saturating_add(1), ll); self.head = self.anchor; }
        else { self.head.col = min(self.head.col.saturating_add(1), ll); }
    }
    pub fn extend_up(&mut self) { if self.mode == SelectionMode::Inactive { self.mode = SelectionMode::Normal; } if self.head.item > 0 { self.head.item -= 1; } }
    pub fn extend_down(&mut self, ti: usize) { if self.mode == SelectionMode::Inactive { self.mode = SelectionMode::Normal; } let m = ti.saturating_sub(1); if self.head.item < m { self.head.item += 1; } }
    pub fn extend_left(&mut self) { if self.mode == SelectionMode::Inactive { self.mode = SelectionMode::Normal; } self.head.col = self.head.col.saturating_sub(1); }
    pub fn extend_right(&mut self, ll: u16) { if self.mode == SelectionMode::Inactive { self.mode = SelectionMode::Normal; } self.head.col = min(self.head.col.saturating_add(1), ll); }
    pub fn select_all(&mut self, ti: usize, ll: u16) { if ti == 0 { self.reset(); return; } self.mode = SelectionMode::Normal; self.anchor = SelectionPosition::new(0, 0); self.head = SelectionPosition::new(ti - 1, ll); }
    pub fn selected_text<'a, F>(&self, gl: F) -> String where F: Fn(usize) -> &'a str {
        let r = match self.normalized_range() { Some(x) => x, None => return String::new() };
        if r.start.item == r.end.item {
            let line: Vec<char> = gl(r.start.item).chars().collect();
            let s = r.start.col as usize; let e = min(r.end.col as usize + 1, line.len());
            if s >= e { return String::new(); }
            line[s..e].iter().collect()
        } else {
            let mut res = String::new();
            let first: Vec<char> = gl(r.start.item).chars().collect();
            let sc = r.start.col as usize;
            if sc < first.len() { for &ch in &first[sc..] { res.push(ch); } }
            res.push('\n');
            for idx in (r.start.item + 1)..r.end.item { res.push_str(gl(idx)); res.push('\n'); }
            let last: Vec<char> = gl(r.end.item).chars().collect();
            let ec = min(r.end.col as usize, last.len().saturating_sub(1));
            for (i, &ch) in last.iter().enumerate() { if i <= ec { res.push(ch); } else { break; } }
            res
        }
    }
    pub fn handle_key(&mut self, key: &KeyEvent, ti: usize, ll: u16) -> bool {
        if key.kind != KeyEventKind::Press { return false; }
        match (key.code, key.shift()) {
            (KeyCode::Up, false) => { self.move_up(); true }
            (KeyCode::Down, false) => { self.move_down(ti); true }
            (KeyCode::Left, false) => { self.move_left(); true }
            (KeyCode::Right, false) => { self.move_right(ll); true }
            (KeyCode::Up, true) => { self.extend_up(); true }
            (KeyCode::Down, true) => { self.extend_down(ti); true }
            (KeyCode::Left, true) => { self.extend_left(); true }
            (KeyCode::Right, true) => { self.extend_right(ll); true }
            (KeyCode::Char('y' | 'Y'), _) | (KeyCode::Enter, _) => { if self.is_active() { self.pending_copy = true; self.exit_selection_mode(); true } else { false } }
            (KeyCode::Escape, _) if self.is_active() => { self.exit_selection_mode(); true }
            _ => false,
        }
    }
    pub fn consume_copy<'a, F>(&mut self, gl: F) -> (String, bool) where F: Fn(usize) -> &'a str {
        if self.pending_copy { self.pending_copy = false; (self.selected_text(gl), true) }
        else { (String::new(), false) }
    }
}

pub fn apply_selection_highlight(
    buf: &mut Buffer, sel: &SelectionState, so: usize, area: Rect, ih: u16, cl: u16, ss: Style,
) {
    let Some(r) = sel.normalized_range() else { return };
    if ss.is_empty() { return; }
    let ih = ih.max(1);
    for y in area.y..area.bottom() {
        let idx = so + ((y - area.y) / ih) as usize;
        if idx < r.start.item || idx > r.end.item { continue; }
        let (cs, ce) = if idx == r.start.item && idx == r.end.item { (r.start.col, r.end.col) }
            else if idx == r.start.item { (r.start.col, cl) }
            else if idx == r.end.item { (0, r.end.col) }
            else { (0, cl) };
        let w = ce.saturating_sub(cs).saturating_add(1).min(area.width.saturating_sub(cs));
        if w == 0 { continue; }
        crate::set_style_area(buf, Rect::new(area.x + cs, y, w, 1), ss);
    }
}

#[must_use] pub fn format_osc52_clipboard(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64_encode(text.as_bytes()))
}

fn base64_encode(input: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for ch in input.chunks(3) {
        let b0 = ch[0] as u32;
        let b1 = ch.get(1).copied().unwrap_or(0) as u32;
        let b2 = ch.get(2).copied().unwrap_or(0) as u32;
        let t = (b0 << 16) | (b1 << 8) | b2;
        out.push(A[((t >> 18) & 0x3F) as usize] as char);
        out.push(A[((t >> 12) & 0x3F) as usize] as char);
        if ch.len() > 1 { out.push(A[((t >> 6) & 0x3F) as usize] as char); }
        else { out.push('='); }
        if ch.len() > 2 { out.push(A[(t & 0x3F) as usize] as char); }
        else { out.push('='); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_core::event::Modifiers;
    use ftui_render::cell::PackedRgba;
    use ftui_render::frame::Frame;
    use ftui_render::grapheme_pool::GraphemePool;
    fn gl(i: usize) -> &'static str { ["Hello, world!", "Second line of text", "Third line here"][i] }

    macro_rules! assert_bg_eq { ($f:ident, $x:expr, $y:expr, $e:expr) => { assert_eq!($f.buffer.get($x, $y).unwrap().bg, $e, "cell ({},{}) bg", $x, $y); }; }

    #[test] fn pos_new() { let p = SelectionPosition::new(5, 3); assert_eq!((p.item, p.col), (5, 3)); }
    #[test] fn pos_def() { assert_eq!(SelectionPosition::default(), SelectionPosition::new(0, 0)); }
    #[test] fn rng_ord() { let r = SelectionRange::normalized(SelectionPosition::new(10,5), SelectionPosition::new(5,3)); assert_eq!((r.start.item, r.end.item), (5,10)); }
    #[test] fn rng_ord_same() { let r = SelectionRange::normalized(SelectionPosition::new(3,8), SelectionPosition::new(3,2)); assert_eq!((r.start.col, r.end.col), (2,8)); }
    #[test] fn rng_eq() { let p = SelectionPosition::new(7,4); let r = SelectionRange::normalized(p,p); assert_eq!(r.start, p); }
    #[test] fn rng_contains_inside() { let r = SelectionRange::normalized(SelectionPosition::new(2,3), SelectionPosition::new(5,7)); assert!(r.contains(SelectionPosition::new(3,0))); }
    #[test] fn rng_contains_outside() { let r = SelectionRange::normalized(SelectionPosition::new(2,3), SelectionPosition::new(5,7)); assert!(!r.contains(SelectionPosition::new(1,0))); assert!(!r.contains(SelectionPosition::new(6,0))); }
    #[test] fn st_new_inactive() { let s = SelectionState::new(); assert!(!s.is_active()); assert_eq!(s.mode, SelectionMode::Inactive); }
    #[test] fn st_default_eq_new() { assert_eq!(SelectionState::default(), SelectionState::new()); }
    #[test] fn st_enter_exit() { let mut s = SelectionState::new(); s.enter_selection_mode(); assert_eq!(s.mode, SelectionMode::Normal); s.exit_selection_mode(); assert_eq!(s.mode, SelectionMode::Inactive); }
    #[test] fn st_reset() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.head = SelectionPosition::new(3,0); s.pending_copy = true; s.reset(); assert_eq!(s, SelectionState::new()); }
    #[test] fn st_norm_none() { assert!(SelectionState::new().normalized_range().is_none()); }
    #[test] fn st_norm_none_eq() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; assert!(s.normalized_range().is_none()); }
    #[test] fn st_norm_correct() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(10,5); s.head = SelectionPosition::new(3,7); let r = s.normalized_range().unwrap(); assert_eq!((r.start.item, r.end.item),(3,10)); }
    #[test] fn st_mv_up() { let mut s = SelectionState::new(); s.anchor = SelectionPosition::new(5,0); s.head = SelectionPosition::new(5,0); s.move_up(); assert_eq!(s.anchor.item, 4); }
    #[test] fn st_mv_down() { let mut s = SelectionState::new(); s.move_down(20); assert_eq!(s.anchor.item, 1); }
    #[test] fn st_mv_up_clamp() { let mut s = SelectionState::new(); s.move_up(); assert_eq!(s.anchor.item, 0); }
    #[test] fn st_mv_down_clamp() { let mut s = SelectionState::new(); s.move_down(1); assert_eq!(s.anchor.item, 0); }
    #[test] fn st_mv_left() { let mut s = SelectionState::new(); s.anchor = SelectionPosition::new(0,5); s.head = SelectionPosition::new(0,5); s.move_left(); assert_eq!(s.anchor.col, 4); }
    #[test] fn st_mv_right() { let mut s = SelectionState::new(); s.move_right(80); assert_eq!(s.anchor.col, 1); }
    #[test] fn st_mv_right_clamp() { let mut s = SelectionState::new(); s.anchor = SelectionPosition::new(0,5); s.head = SelectionPosition::new(0,5); s.move_right(5); assert_eq!(s.anchor.col, 5); }
    #[test] fn st_mv_up_normal() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(10,0); s.head = SelectionPosition::new(10,0); s.move_up(); assert_eq!(s.head.item, 9); assert_eq!(s.anchor.item, 10); }
    #[test] fn st_mv_down_normal() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(10,0); s.head = SelectionPosition::new(10,0); s.move_down(30); assert_eq!(s.head.item, 11); assert_eq!(s.anchor.item, 10); }
    #[test] fn st_ext_up_mode() { let mut s = SelectionState::new(); s.extend_up(); assert_eq!(s.mode, SelectionMode::Normal); }
    #[test] fn st_ext_down_active() { let mut s = SelectionState::new(); s.extend_down(20); assert!(s.is_active()); assert_eq!(s.head.item, 1); assert_eq!(s.anchor.item, 0); }
    #[test] fn st_ext_up_head() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(10,0); s.head = SelectionPosition::new(10,0); s.extend_up(); assert_eq!(s.head.item, 9); assert_eq!(s.anchor.item, 10); }
    #[test] fn st_ext_down_clamp() { let mut s = SelectionState::new(); s.extend_down(1); assert_eq!(s.head.item, 0); }
    #[test] fn st_ext_left() { let mut s = SelectionState::new(); s.extend_left(); assert_eq!(s.head.col, 0); }
    #[test] fn st_ext_right() { let mut s = SelectionState::new(); s.extend_right(10); assert_eq!(s.head.col, 1); assert_eq!(s.anchor.col, 0); }
    #[test] fn st_select_all() { let mut s = SelectionState::new(); s.select_all(10, 80); assert!(s.is_active()); assert_eq!(s.anchor, SelectionPosition::new(0,0)); assert_eq!(s.head, SelectionPosition::new(9,80)); }
    #[test] fn st_select_all_empty() { let mut s = SelectionState::new(); s.select_all(0, 80); assert!(!s.is_active()); }
    #[test] fn st_txt_single() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(0,0); s.head = SelectionPosition::new(0,4); assert_eq!(s.selected_text(gl), "Hello"); }
    #[test] fn st_txt_rev() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(0,4); s.head = SelectionPosition::new(0,0); assert_eq!(s.selected_text(gl), "Hello"); }
    #[test] fn st_txt_multi() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(0,0); s.head = SelectionPosition::new(2,4); let t = s.selected_text(gl); assert!(t.contains("Hello")); assert!(t.contains("Second")); assert!(t.contains("Third")); assert!(t.contains("\n")); }
    #[test] fn st_txt_empty() { assert!(SelectionState::new().selected_text(gl).is_empty()); }
    #[test] fn st_txt_full() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(1,0); s.head = SelectionPosition::new(1,18); assert_eq!(s.selected_text(gl), "Second line of text"); }
    #[test] fn st_txt_single_char() { let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(0,0); s.head = SelectionPosition::new(0,0); assert_eq!(s.selected_text(gl), "H"); }
    #[test] fn st_key_down() { let mut s = SelectionState::new(); let k = KeyEvent::new(KeyCode::Down); assert!(s.handle_key(&k, 10, 80)); assert_eq!(s.head.item, 1); }
    #[test] fn st_key_shift_down() { let mut s = SelectionState::new(); let k = KeyEvent::new(KeyCode::Down).with_modifiers(Modifiers::SHIFT); assert!(s.handle_key(&k, 10, 80)); assert!(s.is_active()); assert_eq!(s.head.item, 1); assert_eq!(s.anchor.item, 0); }
    #[test] fn st_key_y() { let mut s = SelectionState::new(); s.select_all(5, 80); assert!(!s.pending_copy); let k = KeyEvent::new(KeyCode::Char('Y')); assert!(s.handle_key(&k, 5, 80)); assert!(s.pending_copy); assert!(!s.is_active()); }
    #[test] fn st_key_enter() { let mut s = SelectionState::new(); s.select_all(5, 80); let k = KeyEvent::new(KeyCode::Enter); assert!(s.handle_key(&k, 5, 80)); assert!(s.pending_copy); }
    #[test] fn st_key_y_no_sel() { let mut s = SelectionState::new(); let k = KeyEvent::new(KeyCode::Char('Y')); assert!(!s.handle_key(&k, 10, 80)); assert!(!s.pending_copy); }
    #[test] fn st_key_esc_cancel() { let mut s = SelectionState::new(); s.select_all(5, 80); let k = KeyEvent::new(KeyCode::Escape); assert!(s.handle_key(&k, 10, 80)); assert!(!s.is_active()); }
    #[test] fn st_key_esc_noop() { let k = KeyEvent::new(KeyCode::Escape); assert!(!SelectionState::new().handle_key(&k, 10, 80)); }
    #[test] fn st_key_repeat() { let k = KeyEvent::new(KeyCode::Down).with_kind(KeyEventKind::Repeat); assert!(!SelectionState::new().handle_key(&k, 10, 80)); }
    #[test] fn st_consume_ok() { let mut s = SelectionState::new(); s.select_all(3, 10); s.pending_copy = true; let (t, ok) = s.consume_copy(gl); assert!(ok); assert!(!t.is_empty()); assert!(!s.pending_copy); }
    #[test] fn st_consume_none() { assert!(!SelectionState::new().consume_copy(gl).1); }
    #[test] fn hl_single_line() {
        let mut p = GraphemePool::new(); let mut f = Frame::new(20, 3, &mut p);
        let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(0,2); s.head = SelectionPosition::new(0,5);
        apply_selection_highlight(&mut f.buffer, &s, 0, Rect::new(0,0,20,3), 1, 20, Style::new().bg(PackedRgba::rgb(40,40,40)));
        assert_bg_eq!(f, 0, 0, PackedRgba::TRANSPARENT);
        assert_bg_eq!(f, 2, 0, PackedRgba::rgb(40,40,40));
        assert_bg_eq!(f, 5, 0, PackedRgba::rgb(40,40,40));
        assert_bg_eq!(f, 6, 0, PackedRgba::TRANSPARENT);
    }
    #[test] fn hl_inactive() {
        let mut p = GraphemePool::new(); let mut f = Frame::new(10, 3, &mut p);
        apply_selection_highlight(&mut f.buffer, &SelectionState::new(), 0, Rect::new(0,0,10,3), 1, 10, Style::new().bg(PackedRgba::rgb(40,40,40)));
        assert_bg_eq!(f, 0, 0, PackedRgba::TRANSPARENT);
    }
    #[test] fn hl_multi_line() {
        let mut p = GraphemePool::new(); let mut f = Frame::new(20, 5, &mut p);
        let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(1,3); s.head = SelectionPosition::new(2,7);
        apply_selection_highlight(&mut f.buffer, &s, 0, Rect::new(0,0,20,5), 1, 20, Style::new().bg(PackedRgba::rgb(50,50,50)));
        assert_bg_eq!(f, 0, 0, PackedRgba::TRANSPARENT);
        assert_bg_eq!(f, 3, 1, PackedRgba::rgb(50,50,50));
        assert_bg_eq!(f, 7, 2, PackedRgba::rgb(50,50,50));
        assert_bg_eq!(f, 8, 2, PackedRgba::TRANSPARENT);
    }
    #[test] fn hl_item_height() {
        let mut p = GraphemePool::new(); let mut f = Frame::new(10, 6, &mut p);
        let mut s = SelectionState::new(); s.mode = SelectionMode::Normal; s.anchor = SelectionPosition::new(0,2); s.head = SelectionPosition::new(0,5);
        apply_selection_highlight(&mut f.buffer, &s, 0, Rect::new(0,0,10,6), 3, 10, Style::new().bg(PackedRgba::rgb(60,60,60)));
        assert_bg_eq!(f, 2, 0, PackedRgba::rgb(60,60,60));
        assert_bg_eq!(f, 5, 2, PackedRgba::rgb(60,60,60));
        assert_bg_eq!(f, 0, 3, PackedRgba::TRANSPARENT);
    }
    #[test] fn b64_hello() { assert_eq!(base64_encode(b"hello"), "aGVsbG8="); }
    #[test] fn b64_empty() { assert_eq!(base64_encode(b""), ""); }
    #[test] fn b64_abc() { assert_eq!(base64_encode(b"abc"), "YWJj"); }
    #[test] fn osc52() { let r = format_osc52_clipboard("test"); assert!(r.starts_with("\x1b]52;c;")); assert!(r.contains("dGVzdA==")); assert!(r.ends_with('\x07')); }
}
