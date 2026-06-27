#![forbid(unsafe_code)]
//! Unified and split inline diff block widget.

use crate::clear_text_area;
use crate::draw_text_span;
use crate::Widget;
use ftui_core::geometry::Rect;
use ftui_render::cell::PackedRgba;
use ftui_render::frame::Frame;
use ftui_style::Style;

pub const DIFF_ADDED_FG: PackedRgba = PackedRgba::rgb(0xa6, 0xe3, 0xa1);
pub const DIFF_ADDED_BG: PackedRgba = PackedRgba::rgb(0x1e, 0x29, 0x2a);
pub const DIFF_REMOVED_FG: PackedRgba = PackedRgba::rgb(0xf3, 0x8b, 0xa8);
pub const DIFF_REMOVED_BG: PackedRgba = PackedRgba::rgb(0x2d, 0x1a, 0x1e);
pub const DIFF_HUNK_HEADER_FG: PackedRgba = PackedRgba::rgb(0x89, 0xb4, 0xfa);
pub const DIFF_LINENO_FG: PackedRgba = PackedRgba::rgb(0x6c, 0x70, 0x86);
pub const DIFF_HEADER_FG: PackedRgba = PackedRgba::rgb(0xf5, 0xc2, 0xe7);
pub const SHOW_MORE_TEXT: &str = "... (show more)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind { Context, Added, Removed, HunkHeader, FileHeader }

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

impl DiffLine {
    pub fn context(c: impl Into<String>) -> Self {
        Self { kind: DiffLineKind::Context, content: c.into(), old_lineno: None, new_lineno: None }
    }
    pub fn added(c: impl Into<String>) -> Self {
        Self { kind: DiffLineKind::Added, content: c.into(), old_lineno: None, new_lineno: None }
    }
    pub fn removed(c: impl Into<String>) -> Self {
        Self { kind: DiffLineKind::Removed, content: c.into(), old_lineno: None, new_lineno: None }
    }
    pub fn hunk_header(c: impl Into<String>) -> Self {
        Self { kind: DiffLineKind::HunkHeader, content: c.into(), old_lineno: None, new_lineno: None }
    }
    pub fn file_header(c: impl Into<String>) -> Self {
        Self { kind: DiffLineKind::FileHeader, content: c.into(), old_lineno: None, new_lineno: None }
    }
    pub fn with_linenos(mut self, old: u32, new: u32) -> Self {
        self.old_lineno = Some(old); self.new_lineno = Some(new); self
    }
}

#[derive(Debug, Clone)]
pub struct DiffHunk { pub header: String, pub lines: Vec<DiffLine> }

impl DiffHunk {
    pub fn new(header: impl Into<String>, lines: Vec<DiffLine>) -> Self { Self { header: header.into(), lines } }
    pub fn total_lines(&self) -> usize { 1 + self.lines.len() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffMode { #[default] Unified, SideBySide }

#[derive(Debug, Clone)]
pub struct DiffBlock<'a> {
    header: Option<&'a str>,
    hunks: Vec<DiffHunk>,
    max_lines: usize,
    show_line_numbers: bool,
    mode: DiffMode,
    collapsed: bool,
    style_added_fg: Option<PackedRgba>,
    style_added_bg: Option<PackedRgba>,
    style_removed_fg: Option<PackedRgba>,
    style_removed_bg: Option<PackedRgba>,
    style_hunk_header_fg: Option<PackedRgba>,
    style_header_fg: Option<PackedRgba>,
}

impl Default for DiffBlock<'_> {
    fn default() -> Self {
        Self {
            header: None, hunks: Vec::new(), max_lines: usize::MAX,
            show_line_numbers: false, mode: DiffMode::Unified, collapsed: false,
            style_added_fg: None, style_added_bg: None, style_removed_fg: None,
            style_removed_bg: None, style_hunk_header_fg: None, style_header_fg: None,
        }
    }
}

impl<'a> DiffBlock<'a> {
    pub fn new() -> Self { Self::default() }
    pub fn header(mut self, t: &'a str) -> Self { self.header = Some(t); self }
    pub fn hunks(mut self, h: Vec<DiffHunk>) -> Self { self.hunks = h; self }
    pub fn hunk(mut self, h: DiffHunk) -> Self { self.hunks.push(h); self }
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    pub fn show_line_numbers(mut self, s: bool) -> Self { self.show_line_numbers = s; self }
    pub fn mode(mut self, m: DiffMode) -> Self { self.mode = m; self }
    pub fn collapsed(mut self, c: bool) -> Self { self.collapsed = c; self }
    pub fn added_fg(mut self, c: PackedRgba) -> Self { self.style_added_fg = Some(c); self }
    pub fn added_bg(mut self, c: PackedRgba) -> Self { self.style_added_bg = Some(c); self }
    pub fn removed_fg(mut self, c: PackedRgba) -> Self { self.style_removed_fg = Some(c); self }
    pub fn removed_bg(mut self, c: PackedRgba) -> Self { self.style_removed_bg = Some(c); self }
    pub fn hunk_header_fg(mut self, c: PackedRgba) -> Self { self.style_hunk_header_fg = Some(c); self }
}
impl Widget for DiffBlock<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() || area.height == 0 { return; }
        let total = (if self.header.is_some() { 1_usize } else { 0 })
            + self.hunks.iter().map(|h| h.total_lines()).sum::<usize>();
        let max_y = area.bottom();
        let mut y = area.y;
        let mut rendered: usize = 0;
        let right = area.right();
        if let Some(hdr) = self.header {
            if y >= max_y { return; }
            clear_text_area(frame, Rect::new(area.x, y, area.width, 1), Style::default());
            let style = Style::new().fg(self.style_header_fg.unwrap_or(DIFF_HEADER_FG));
            draw_text_span(frame, area.x, y, hdr, style, right);
            y += 1; rendered += 1;
        }
        let show = if self.collapsed { 0 } else { self.max_lines.min((max_y - y) as usize) };
        for hunk in &self.hunks {
            if rendered >= show || y >= max_y { break; }
            clear_text_area(frame, Rect::new(area.x, y, area.width, 1), Style::default());
            let hdr_style = Style::new().fg(self.style_hunk_header_fg.unwrap_or(DIFF_HUNK_HEADER_FG));
            draw_text_span(frame, area.x, y, &hunk.header, hdr_style, right);
            y += 1; rendered += 1;
            if y >= max_y || rendered >= show { break; }
            for line in &hunk.lines {
                if rendered >= show || y >= max_y { break; }
                clear_text_area(frame, Rect::new(area.x, y, area.width, 1), Style::default());
                let style = match line.kind {
                    DiffLineKind::Context => Style::default(),
                    DiffLineKind::Added => Style::new().fg(self.style_added_fg.unwrap_or(DIFF_ADDED_FG)).bg(self.style_added_bg.unwrap_or(DIFF_ADDED_BG)),
                    DiffLineKind::Removed => Style::new().fg(self.style_removed_fg.unwrap_or(DIFF_REMOVED_FG)).bg(self.style_removed_bg.unwrap_or(DIFF_REMOVED_BG)),
                    DiffLineKind::HunkHeader => Style::new().fg(self.style_hunk_header_fg.unwrap_or(DIFF_HUNK_HEADER_FG)),
                    DiffLineKind::FileHeader => Style::new().fg(self.style_header_fg.unwrap_or(DIFF_HEADER_FG)),
                };
                let prefix = match line.kind {
                    DiffLineKind::Added => "+", DiffLineKind::Removed => "-",
                    DiffLineKind::HunkHeader => "@@ ", _ => " ",
                };
                let mut cursor = area.x;
                if self.show_line_numbers {
                    let a = match line.old_lineno { Some(n) => format!("{:>4}", n), None => "    ".to_string() };
                    let b = match line.new_lineno { Some(n) => format!("{:>4}", n), None => "    ".to_string() };
                    let gutter = format!("{}|{} ", a, b);
                    cursor = draw_text_span(frame, cursor, y, &gutter, Style::new().fg(DIFF_LINENO_FG).dim(), right);
                }
                cursor = draw_text_span(frame, cursor, y, prefix, style, right);
                draw_text_span(frame, cursor, y, &line.content, style, right);
                y += 1; rendered += 1;
            }
        }
        if rendered < total && y < max_y && !self.collapsed {
            clear_text_area(frame, Rect::new(area.x, y, area.width, 1), Style::default());
            draw_text_span(frame, area.x, y, SHOW_MORE_TEXT, Style::new().fg(DIFF_HUNK_HEADER_FG).dim(), right);
        }
    }
    fn is_essential(&self) -> bool { true }
}
impl DiffBlock<'_> {
    pub fn parse_unified_diff(input: &str) -> (String, Vec<DiffHunk>) {
        let mut header = String::new();
        let mut hunks = Vec::new();
        let mut cur_hdr: Option<String> = None;
        let mut cur_lines: Vec<DiffLine> = Vec::new();
        for raw_line in input.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() { continue; }
            if line.starts_with("@@") {
                if let Some(h) = cur_hdr.take() { hunks.push(DiffHunk::new(h, std::mem::take(&mut cur_lines))); }
                cur_hdr = Some(line.to_string());
            } else if cur_hdr.is_some() {
                let (kind, content) = if line.starts_with('+') { (DiffLineKind::Added, &line[1..]) }
                else if line.starts_with('-') { (DiffLineKind::Removed, &line[1..]) }
                else if line.starts_with(' ') { (DiffLineKind::Context, &line[1..]) }
                else { (DiffLineKind::Context, line) };
                cur_lines.push(DiffLine { kind, content: content.to_string(), old_lineno: None, new_lineno: None });
            } else {
                if !header.is_empty() { header.push(' '); }
                header.push_str(line);
            }
        }
        if let Some(h) = cur_hdr.take() { hunks.push(DiffHunk::new(h, std::mem::take(&mut cur_lines))); }
        (header, hunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::budget::DegradationLevel;
    use ftui_render::grapheme_pool::GraphemePool;

    fn render_to(w: &DiffBlock, wd: u16, ht: u16) -> Vec<String> {
        let mut p = GraphemePool::new();
        let mut f = Frame::new(wd, ht, &mut p);
        w.render(Rect::new(0, 0, wd, ht), &mut f);
        (0..ht).map(|y| (0..wd).map(|x| f.buffer.get(x, y).and_then(|c| c.content.as_char()).unwrap_or(' ')).collect()).collect()
    }

    fn contains(lines: &[String], idx: usize, needle: &str) {
        assert!(idx < lines.len(), "idx {idx} >= {}", lines.len());
        assert!(lines[idx].contains(needle), "line {idx}: {:?} !contains {:?}", lines[idx], needle);
    }

    #[test] fn diff_line_context() { let l = DiffLine::context("h"); assert_eq!(l.kind, DiffLineKind::Context); }
    #[test] fn diff_line_added() { let l = DiffLine::added("w"); assert_eq!(l.kind, DiffLineKind::Added); }
    #[test] fn diff_line_removed() { let l = DiffLine::removed("o"); assert_eq!(l.kind, DiffLineKind::Removed); }
    #[test] fn diff_line_hunk_header() { let l = DiffLine::hunk_header("@@ -1,3 +1,4 @@"); assert_eq!(l.kind, DiffLineKind::HunkHeader); }
    #[test] fn diff_line_file_header() { let l = DiffLine::file_header("diff --git a/src/main.rs b/src/main.rs"); assert_eq!(l.kind, DiffLineKind::FileHeader); }
    #[test] fn diff_line_with_linenos() { let l = DiffLine::context("t").with_linenos(10, 20); assert_eq!(l.old_lineno, Some(10)); assert_eq!(l.new_lineno, Some(20)); }
    #[test] fn diff_hunk_new() { let h = DiffHunk::new("@@ -1,3 +1,4 @@", vec![DiffLine::context("a"), DiffLine::added("b")]); assert_eq!(h.total_lines(), 3); }
    #[test] fn diff_hunk_total_lines() { let h = DiffHunk::new("@", vec![DiffLine::context("x"); 5]); assert_eq!(h.total_lines(), 6); }
    #[test] fn builder_defaults() { let b = DiffBlock::new(); assert!(b.hunks.is_empty()); assert_eq!(b.max_lines, usize::MAX); assert!(!b.show_line_numbers); }
    #[test] fn builder_header() { assert_eq!(DiffBlock::new().header("t").header, Some("t")); }
    #[test] fn builder_max_lines() { assert_eq!(DiffBlock::new().max_lines(10).max_lines, 10); }
    #[test] fn builder_show_line_numbers() { assert!(DiffBlock::new().show_line_numbers(true).show_line_numbers); }
    #[test] fn builder_collapsed() { assert!(DiffBlock::new().collapsed(true).collapsed); }
    #[test] fn builder_color_overrides() { let b = DiffBlock::new().added_fg(PackedRgba::rgb(1,2,3)); assert_eq!(b.style_added_fg, Some(PackedRgba::rgb(1,2,3))); }
    #[test] fn diff_mode_default() { assert_eq!(DiffMode::default(), DiffMode::Unified); }
    #[test] fn diff_mode_distinct() { assert_ne!(DiffMode::Unified, DiffMode::SideBySide); }
    #[test] fn render_empty_area() { DiffBlock::new().render(Rect::new(0,0,0,0), &mut Frame::new(1,1,&mut GraphemePool::new())); }
    #[test] fn render_unified_basic() {
        let w = DiffBlock::new().hunks(vec![DiffHunk::new("@@ -1,3 +1,4 @@", vec![DiffLine::context("fn main() {"), DiffLine::removed("    let x = 1;"), DiffLine::added("    let x = 42;")])]);
        let lines = render_to(&w, 40, 10);
        contains(&lines, 0, "@@"); contains(&lines, 1, " fn main() {"); contains(&lines, 2, "-    let x = 1;"); contains(&lines, 3, "+    let x = 42;");
    }
    #[test] fn render_header() {
        let w = DiffBlock::new().header("diff --git a/src/main.rs b/src/main.rs").hunks(vec![DiffHunk::new("@@ -1 +1 @@", vec![DiffLine::context("a")])]);
        let lines = render_to(&w, 50, 10); contains(&lines, 0, "diff --git");
    }
    #[test] fn render_collapsed() {
        let w = DiffBlock::new().collapsed(true).hunks(vec![DiffHunk::new("@@ -1 +1 @@", vec![DiffLine::context("x")])]);
        for l in render_to(&w, 20, 5) { assert_eq!(l.trim(), ""); }
    }
    #[test] fn render_multiple_hunks() {
        let w = DiffBlock::new().hunks(vec![DiffHunk::new("@@ -1 +1 @@", vec![DiffLine::context("a")]), DiffHunk::new("@@ -2 +2 @@", vec![DiffLine::context("b")])]);
        let lines = render_to(&w, 20, 10); contains(&lines, 0, "@@ -1"); contains(&lines, 2, "@@ -2");
    }
    #[test] fn render_truncate_area_height() {
        let w = DiffBlock::new().hunks(vec![DiffHunk::new("@@ -1,5 +1,5 @@", vec![DiffLine::context("a"),DiffLine::context("b"),DiffLine::context("c"),DiffLine::context("d")])]);
        assert_eq!(render_to(&w, 20, 3).len(), 3);
    }
    #[test] fn render_no_hunks() { for l in render_to(&DiffBlock::new(), 20, 5) { assert_eq!(l.trim(), ""); } }
    #[test] fn render_show_more() {
        let w = DiffBlock::new().max_lines(1).hunks(vec![DiffHunk::new("@@ -1 +1 @@", vec![DiffLine::context("a")])]);
        let lines = render_to(&w, 30, 10); contains(&lines, 1, "show more");
    }
    #[test] fn render_line_numbers() {
        let w = DiffBlock::new().show_line_numbers(true).hunks(vec![DiffHunk::new("@@ -1,2 +1,2 @@", vec![DiffLine::context(" a").with_linenos(1,1)])]);
        let lines = render_to(&w, 30, 10); contains(&lines, 1, "|");
    }
    #[test] fn is_essential_true() { assert!(DiffBlock::new().is_essential()); }
    #[test] fn render_degradation() {
        let mut p = GraphemePool::new(); let mut f = Frame::new(10, 5, &mut p);
        f.buffer.degradation = DegradationLevel::NoStyling;
        DiffBlock::new().hunks(vec![DiffHunk::new("@@ -1 +1 @@", vec![DiffLine::added("x")])]).render(Rect::new(0,0,10,5), &mut f);
    }
    #[test] fn parse_simple() {
        let (h, hunks) = DiffBlock::parse_unified_diff("@@ -1,3 +1,4 @@\n fn main() {\n-    let x = 1;\n+    let x = 42;\n}\n");
        assert!(h.is_empty()); assert_eq!(hunks.len(), 1); assert_eq!(hunks[0].lines.len(), 4);
    }
    #[test] fn parse_with_header() {
        let (h, hunks) = DiffBlock::parse_unified_diff("diff --git a/x b/x\n@@ -1 +1 @@\n-a\n+b\n");
        assert!(h.contains("diff --git")); assert_eq!(hunks[0].lines[0].kind, DiffLineKind::Removed);
    }
    #[test] fn parse_multiple_hunks() {
        let (_, hunks) = DiffBlock::parse_unified_diff("@@ -1,2 +1,2 @@\n a\n b\n@@ -5,2 +5,3 @@\n c\n+d\n");
        assert_eq!(hunks.len(), 2);
    }
    #[test] fn parse_empty() { let (h, hunks) = DiffBlock::parse_unified_diff(""); assert!(h.is_empty()); assert!(hunks.is_empty()); }
    #[test] fn parse_only_header() { let (h, hunks) = DiffBlock::parse_unified_diff("diff --git a/x b/x"); assert!(h.contains("diff")); assert!(hunks.is_empty()); }
    #[test] fn parse_crlf() { let (_, hunks) = DiffBlock::parse_unified_diff("@@ -1 +1 @@\r\n-a\r\n+b\r\n"); assert_eq!(hunks[0].lines[0].content, "a"); }
    #[test] fn diff_line_debug() { let d = format!("{:?}", DiffLine::added("test")); assert!(d.contains("Added")); }
    #[test] fn diff_line_clone() { let a = DiffLine::context("x").with_linenos(1,2); let b = a.clone(); assert_eq!(a.content, b.content); }
    #[test] fn diff_hunk_debug() { let d = format!("{:?}", DiffHunk::new("@@ -1 +1 @@", vec![])); assert!(d.contains("@@ -1 +1 @@")); }
    #[test] fn diff_hunk_clone() { let a = DiffHunk::new("h", vec![DiffLine::context("c")]); let b = a.clone(); assert_eq!(a.lines.len(), b.lines.len()); }
    #[test] fn diff_line_kind_debug() { assert_eq!(format!("{:?}", DiffLineKind::Added), "Added"); }
    #[test] fn diff_line_kind_distinct() { assert_ne!(DiffLineKind::Added, DiffLineKind::Removed); }
    #[test] fn render_exact_fit() { assert_eq!(render_to(&DiffBlock::new().hunks(vec![DiffHunk::new("@@ -1,2 +1,2 @@", vec![DiffLine::context("a")])]), 10, 2).len(), 2); }
    #[test] fn render_zero_height_no_panic() { DiffBlock::new().render(Rect::new(0,0,10,0), &mut Frame::new(10,1,&mut GraphemePool::new())); }
    #[test] fn render_wide_line_no_panic() {
        let mut p = GraphemePool::new(); let mut f = Frame::new(10, 5, &mut p);
        DiffBlock::new().hunks(vec![DiffHunk::new("@@ -1 +1 @@", vec![DiffLine::added("x".repeat(200))])]).render(Rect::new(0,0,10,5), &mut f);
    }
}
