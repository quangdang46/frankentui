#![forbid(unsafe_code)]

//! Status line widget for agent harness UIs.
//!
//! Provides a horizontal status bar with left, center, and right regions
//! that can contain text, spinners, progress indicators, and key hints.
//!
//! Items support priorities for progressive collapse at narrow widths:
//! lower-priority items are removed first. Critical items are never removed.

use crate::{Widget, apply_style, draw_text_span};
use ftui_core::geometry::Rect;
use ftui_render::cell::Cell;
use ftui_render::frame::Frame;
use ftui_style::Style;
use ftui_text::display_width;

/// Priority level for a status line item.
///
/// Controls which items are removed first when the status line is too narrow
/// to fit all content. Items are removed in order of lowest priority first;
/// items with the same priority are removed in reverse addition order.
///
/// The default priority is `Normal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Never removed during progressive collapse.
    Critical = 0,
    /// High importance, removed only after Normal and Low are exhausted.
    High = 1,
    /// Default priority for all items.
    Normal = 2,
    /// Lowest importance, removed first.
    Low = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// The underlying kind of a status line item.
#[derive(Debug, Clone)]
enum StatusItemKind<'a> {
    /// Plain text.
    Text(&'a str),
    /// A spinner showing activity (references spinner state by index).
    Spinner(usize),
    /// A progress indicator showing current/total.
    Progress {
        /// Current progress value.
        current: u64,
        /// Total progress value.
        total: u64,
    },
    /// A key hint showing a key and its action.
    KeyHint {
        /// Key binding label.
        key: &'a str,
        /// Description of the action.
        action: &'a str,
    },
    /// A flexible spacer that expands to fill available space.
    Spacer,
}

/// An item that can be displayed in the status line.
///
/// Each item has a [`Priority`] that controls whether it is removed during
/// progressive collapse when the terminal is too narrow to fit all content.
#[derive(Debug, Clone)]
pub struct StatusItem<'a> {
    kind: StatusItemKind<'a>,
    priority: Priority,
}

impl<'a> StatusItem<'a> {
    /// Create a text item with default priority (`Normal`).
    pub const fn text(s: &'a str) -> Self {
        Self {
            kind: StatusItemKind::Text(s),
            priority: Priority::Normal,
        }
    }

    /// Create a key hint item with default priority (`Normal`).
    pub const fn key_hint(key: &'a str, action: &'a str) -> Self {
        Self {
            kind: StatusItemKind::KeyHint { key, action },
            priority: Priority::Normal,
        }
    }

    /// Create a progress item with default priority (`Normal`).
    pub const fn progress(current: u64, total: u64) -> Self {
        Self {
            kind: StatusItemKind::Progress { current, total },
            priority: Priority::Normal,
        }
    }

    /// Create a spacer item with default priority (`Normal`).
    pub const fn spacer() -> Self {
        Self {
            kind: StatusItemKind::Spacer,
            priority: Priority::Normal,
        }
    }

    /// Create a spinner item with default priority (`Normal`).
    pub const fn spinner(idx: usize) -> Self {
        Self {
            kind: StatusItemKind::Spinner(idx),
            priority: Priority::Normal,
        }
    }

    /// Set the priority level for collapse behavior.
    #[must_use]
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Get the priority of this item.
    pub fn priority(&self) -> Priority {
        self.priority
    }

    /// Calculate the display width of this item.
    fn width(&self) -> usize {
        match &self.kind {
            StatusItemKind::Text(s) => display_width(s),
            StatusItemKind::Spinner(_) => 1,
            StatusItemKind::Progress { current, total } => {
                let pct = current.saturating_mul(100).checked_div(*total).unwrap_or(0);
                format!("{pct}%").len()
            }
            StatusItemKind::KeyHint { key, action } => {
                display_width(key) + 1 + display_width(action)
            }
            StatusItemKind::Spacer => 0,
        }
    }

    /// Render this item to a string.
    fn render_to_string(&self) -> String {
        match &self.kind {
            StatusItemKind::Text(s) => (*s).to_string(),
            StatusItemKind::Spinner(idx) => {
                const FRAMES: &[char] = &['\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}', '\u{2827}', '\u{2807}', '\u{280F}'];
                FRAMES[*idx % FRAMES.len()].to_string()
            }
            StatusItemKind::Progress { current, total } => {
                let pct = current.saturating_mul(100).checked_div(*total).unwrap_or(0);
                format!("{pct}%")
            }
            StatusItemKind::KeyHint { key, action } => {
                format!("{key} {action}")
            }
            StatusItemKind::Spacer => String::new(),
        }
    }
}

/// A status line widget with left, center, and right regions.
#[derive(Debug, Clone, Default)]
pub struct StatusLine<'a> {
    left: Vec<StatusItem<'a>>,
    center: Vec<StatusItem<'a>>,
    right: Vec<StatusItem<'a>>,
    style: Style,
    separator: &'a str,
}

impl<'a> StatusLine<'a> {
    /// Create a new empty status line.
    pub fn new() -> Self {
        Self {
            left: Vec::new(),
            center: Vec::new(),
            right: Vec::new(),
            style: Style::default(),
            separator: " ",
        }
    }

    /// Add an item to the left region.
    #[must_use]
    pub fn left(mut self, item: StatusItem<'a>) -> Self {
        self.left.push(item);
        self
    }

    /// Add an item to the center region.
    #[must_use]
    pub fn center(mut self, item: StatusItem<'a>) -> Self {
        self.center.push(item);
        self
    }

    /// Add an item to the right region.
    #[must_use]
    pub fn right(mut self, item: StatusItem<'a>) -> Self {
        self.right.push(item);
        self
    }

    /// Set the overall style for the status line.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the separator between items (default: `` " ``).
    #[must_use]
    pub fn separator(mut self, separator: &'a str) -> Self {
        self.separator = separator;
        self
    }

    /// Calculate total fixed width (non-spacers, with separators between non-spacers).
    fn items_fixed_width(&self, items: &[StatusItem]) -> usize {
        let sep_width = display_width(self.separator);
        let mut width = 0usize;
        let mut prev_item = false;

        for item in items {
            if matches!(item.kind, StatusItemKind::Spacer) {
                prev_item = false;
                continue;
            }

            if prev_item {
                width += sep_width;
            }
            width += item.width();
            prev_item = true;
        }

        width
    }

    /// Count flexible spacers in an item list.
    fn spacer_count(&self, items: &[StatusItem]) -> usize {
        items
            .iter()
            .filter(|item| matches!(item.kind, StatusItemKind::Spacer))
            .count()
    }

    /// Collapse a region's items to fit within `available_width`.
    ///
    /// Iteratively removes the lowest-priority items until the remaining
    /// content fits. Items with the same priority are removed in reverse
    /// addition order (last-added first). Items with Priority::Critical
    /// are never removed. Spacers are not removed (they have zero fixed width).
    fn collapse_region(&self, items: &[StatusItem<'a>], available_width: usize) -> Vec<StatusItem<'a>> {
        let total = self.items_fixed_width(items);
        if total <= available_width || items.is_empty() {
            return items.to_vec();
        }

        // Collect indices of removable items (non-Critical, non-Spacer)
        let mut candidates: Vec<usize> = (0..items.len())
            .filter(|&i| {
                items[i].priority != Priority::Critical
                    && !matches!(items[i].kind, StatusItemKind::Spacer)
            })
            .collect();

        // Sort: lowest priority first, then reverse addition order
        candidates.sort_by(|&a, &b| {
            items[b]
                .priority
                .cmp(&items[a].priority)
                .then(b.cmp(&a))
        });

        let mut keep = vec![true; items.len()];
        for idx in candidates {
            keep[idx] = false;
            let filtered: Vec<StatusItem> = items
                .iter()
                .enumerate()
                .filter(|(i, _)| keep[*i])
                .map(|(_, item)| item.clone())
                .collect();
            if self.items_fixed_width(&filtered) <= available_width {
                return filtered;
            }
        }

        items
            .iter()
            .enumerate()
            .filter(|(i, _)| keep[*i])
            .map(|(_, item)| item.clone())
            .collect()
    }

    /// Render a list of items starting at x position.
    fn render_items(
        &self,
        frame: &mut Frame,
        items: &[StatusItem],
        mut x: u16,
        y: u16,
        max_x: u16,
        style: Style,
    ) -> u16 {
        let available = max_x.saturating_sub(x) as usize;
        let fixed_width = self.items_fixed_width(items);
        let spacers = self.spacer_count(items);
        let extra = available.saturating_sub(fixed_width);
        let per_spacer = extra.checked_div(spacers).unwrap_or(0);
        let mut remainder = extra.checked_rem(spacers).unwrap_or(0);
        let mut prev_item = false;

        for item in items {
            if x >= max_x {
                break;
            }

            if matches!(item.kind, StatusItemKind::Spacer) {
                let mut space = per_spacer;
                if remainder > 0 {
                    space += 1;
                    remainder -= 1;
                }
                let advance = (space as u16).min(max_x.saturating_sub(x));
                x = x.saturating_add(advance);
                prev_item = false;
                continue;
            }

            if prev_item && !self.separator.is_empty() {
                x = draw_text_span(frame, x, y, self.separator, style, max_x);
                if x >= max_x {
                    break;
                }
            }

            let text = item.render_to_string();
            x = draw_text_span(frame, x, y, &text, style, max_x);
            prev_item = true;
        }

        x
    }
}

impl Widget for StatusLine<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "widget_render",
            widget = "StatusLine",
            x = area.x,
            y = area.y,
            w = area.width,
            h = area.height
        )
        .entered();

        if area.is_empty() || area.height < 1 {
            return;
        }

        let deg = frame.buffer.degradation;
        let style = if deg.apply_styling() { self.style } else { Style::default() };

        for x in area.x..area.right() {
            let mut cell = Cell::from_char(' ');
            apply_style(&mut cell, style);
            frame.buffer.set_fast(x, area.y, cell);
        }

        let width = area.width as usize;
        let collapsed_left = self.collapse_region(&self.left, width);
        let collapsed_center = self.collapse_region(&self.center, width);
        let collapsed_right = self.collapse_region(&self.right, width);

        let left_width = self.items_fixed_width(&collapsed_left);
        let center_width = self.items_fixed_width(&collapsed_center);
        let right_width = self.items_fixed_width(&collapsed_right);
        let center_spacers = self.spacer_count(&collapsed_center);

        let left_x = area.x;
        let right_x = area.right().saturating_sub(right_width as u16).max(area.x);
        let available_center = width.saturating_sub(left_width).saturating_sub(right_width);
        let center_target_width = if center_width > 0 && center_spacers > 0 {
            available_center
        } else {
            center_width
        };
        let center_x = if center_width > 0 || center_spacers > 0 {
            let center_start =
                left_width + available_center.saturating_sub(center_target_width) / 2;
            area.x.saturating_add(center_start as u16)
        } else {
            area.x
        };

        let center_can_render = (center_width > 0 || center_spacers > 0)
            && center_x + center_target_width as u16 <= right_x;
        let left_max_x = if center_can_render { center_x } else { right_x };

        if !self.left.is_empty() {
            self.render_items(frame, &collapsed_left, left_x, area.y, left_max_x, style);
        }
        if center_can_render {
            self.render_items(frame, &collapsed_center, center_x, area.y, right_x, style);
        }
        if !self.right.is_empty() {
            self.render_items(frame, &collapsed_right, right_x, area.y, area.right(), style);
        }
    }

    fn is_essential(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::budget::DegradationLevel;
    use ftui_render::buffer::Buffer;
    use ftui_render::cell::PackedRgba;
    use ftui_render::grapheme_pool::GraphemePool;

    fn row_string(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| {
                buf.get(x, y)
                    .and_then(|c| c.content.as_char())
                    .unwrap_or(' ')
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn row_full(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| {
                buf.get(x, y)
                    .and_then(|c| c.content.as_char())
                    .unwrap_or(' ')
            })
            .collect()
    }

    #[test]
    fn empty_status_line() {
        let status = StatusLine::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.is_empty() || s.chars().all(|c| c == ' '));
    }

    #[test]
    fn left_only() {
        let status = StatusLine::new().left(StatusItem::text("[INSERT]"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.starts_with("[INSERT]"), "Got: '{}'", s);
    }

    #[test]
    fn right_only() {
        let status = StatusLine::new().right(StatusItem::text("Ln 42"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.ends_with("Ln 42"), "Got: '{}'", s);
    }

    #[test]
    fn center_only() {
        let status = StatusLine::new().center(StatusItem::text("file.rs"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.contains("file.rs"), "Got: '{}'", s);
        let pos = s.find("file.rs").unwrap();
        assert!(pos > 2 && pos < 15, "Not centered, pos={}, got: '{}'", pos, s);
    }

    #[test]
    fn all_three_regions() {
        let status = StatusLine::new()
            .left(StatusItem::text("L"))
            .center(StatusItem::text("C"))
            .right(StatusItem::text("R"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.starts_with("L"), "Got: '{}'", s);
        assert!(s.ends_with("R"), "Got: '{}'", s);
        assert!(s.contains("C"), "Got: '{}'", s);
    }

    #[test]
    fn key_hint() {
        let status = StatusLine::new().left(StatusItem::key_hint("^C", "Quit"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.contains("^C Quit"), "Got: '{}'", s);
    }

    #[test]
    fn progress() {
        let status = StatusLine::new().left(StatusItem::progress(50, 100));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.contains("50%"), "Got: '{}'", s);
    }

    #[test]
    fn multiple_items_left() {
        let status = StatusLine::new()
            .left(StatusItem::text("A"))
            .left(StatusItem::text("B"))
            .left(StatusItem::text("C"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.starts_with("A B C"), "Got: '{}'", s);
    }

    #[test]
    fn custom_separator() {
        let status = StatusLine::new()
            .separator(" | ")
            .left(StatusItem::text("A"))
            .left(StatusItem::text("B"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.contains("A | B"), "Got: '{}'", s);
    }

    #[test]
    fn spacer_expands_and_skips_separators() {
        let status = StatusLine::new()
            .separator(" | ")
            .left(StatusItem::text("L"))
            .left(StatusItem::spacer())
            .left(StatusItem::text("R"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        status.render(Rect::new(0, 0, 10, 1), &mut frame);
        let row = row_full(&frame.buffer, 0, 10);
        let chars: Vec<char> = row.chars().collect();
        assert_eq!(chars[0], 'L');
        assert_eq!(chars[9], 'R');
        assert!(!row.contains('|'), "Spacer should skip separators, got: '{}'", row);
    }

    #[test]
    fn style_applied() {
        let fg = PackedRgba::rgb(255, 0, 0);
        let status = StatusLine::new()
            .style(Style::new().fg(fg))
            .left(StatusItem::text("X"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        status.render(Rect::new(0, 0, 10, 1), &mut frame);
        assert_eq!(frame.buffer.get(0, 0).unwrap().fg, fg);
    }

    #[test]
    fn is_essential() {
        assert!(StatusLine::new().is_essential());
    }

    #[test]
    fn zero_area_no_panic() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        StatusLine::new().left(StatusItem::text("Test")).render(Rect::new(0, 0, 0, 0), &mut frame);
    }

    #[test]
    fn spinner_renders_braille_char() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        StatusLine::new().left(StatusItem::spinner(0)).render(Rect::new(0, 0, 10, 1), &mut frame);
        assert_eq!(frame.buffer.get(0, 0).and_then(|c| c.content.as_char()).unwrap(), '\u{280B}');
    }

    #[test]
    fn spinner_cycles_through_frames() {
        let item0 = StatusItem::spinner(0);
        let item10 = StatusItem::spinner(10);
        assert_eq!(item0.render_to_string(), item10.render_to_string());
        assert_ne!(item0.render_to_string(), StatusItem::spinner(1).render_to_string());
    }

    #[test]
    fn spinner_width_is_one() {
        assert_eq!(StatusItem::spinner(5).width(), 1);
    }

    #[test]
    fn progress_zero_total_shows_zero_percent() {
        assert_eq!(StatusItem::progress(50, 0).render_to_string(), "0%");
    }

    #[test]
    fn spacer_width_is_zero() {
        assert_eq!(StatusItem::spacer().width(), 0);
    }

    #[test]
    fn spacer_render_to_string_is_empty() {
        assert_eq!(StatusItem::spacer().render_to_string(), "");
    }

    #[test]
    fn status_line_default_is_empty() {
        let status = StatusLine::default();
        assert!(status.left.is_empty());
        assert!(status.center.is_empty());
        assert!(status.right.is_empty());
        assert_eq!(status.separator, "");
    }

    #[test]
    fn multiple_items_right() {
        let status = StatusLine::new()
            .right(StatusItem::text("X"))
            .right(StatusItem::text("Y"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        status.render(Rect::new(0, 0, 20, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 20);
        assert!(s.contains("X Y"), "Got: '{}'", s);
    }

    #[test]
    fn key_hint_width() {
        assert_eq!(StatusItem::key_hint("^C", "Quit").width(), 7);
    }

    #[test]
    fn progress_full_hundred_percent() {
        assert_eq!(StatusItem::progress(100, 100).render_to_string(), "100%");
    }

    #[test]
    fn truncation_when_too_narrow() {
        let status = StatusLine::new()
            .left(StatusItem::text("VERYLONGTEXT"))
            .right(StatusItem::text("R"));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        status.render(Rect::new(0, 0, 10, 1), &mut frame);
        let s = row_string(&frame.buffer, 0, 10);
        assert!(!s.is_empty(), "Got empty string");
    }

    #[test]
    fn skeleton_empty_status_line_clears_stale_row() {
        let populated = StatusLine::new().left(StatusItem::text("BUSY"));
        let empty = StatusLine::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(12, 1, &mut pool);
        populated.render(Rect::new(0, 0, 12, 1), &mut frame);
        frame.buffer.degradation = DegradationLevel::Skeleton;
        empty.render(Rect::new(0, 0, 12, 1), &mut frame);
        assert_eq!(row_full(&frame.buffer, 0, 12), " ".repeat(12));
    }

    // -----------------------------------------------------------------------
    // Priority tests
    // -----------------------------------------------------------------------

    #[test]
    fn priority_default_is_normal() {
        assert_eq!(Priority::default(), Priority::Normal);
    }

    #[test]
    fn priority_ordering_critical_lowest() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Normal);
        assert!(Priority::Normal < Priority::Low);
    }

    #[test]
    fn priority_discriminants() {
        assert_eq!(Priority::Critical as u8, 0);
        assert_eq!(Priority::High as u8, 1);
        assert_eq!(Priority::Normal as u8, 2);
        assert_eq!(Priority::Low as u8, 3);
    }

    #[test]
    fn text_default_priority() {
        assert_eq!(StatusItem::text("hello").priority(), Priority::Normal);
    }

    #[test]
    fn key_hint_default_priority() {
        assert_eq!(StatusItem::key_hint("^C", "Quit").priority(), Priority::Normal);
    }

    #[test]
    fn progress_default_priority() {
        assert_eq!(StatusItem::progress(50, 100).priority(), Priority::Normal);
    }

    #[test]
    fn spinner_default_priority() {
        assert_eq!(StatusItem::spinner(3).priority(), Priority::Normal);
    }

    #[test]
    fn with_priority_sets_and_returns_priority() {
        assert_eq!(
            StatusItem::text("critical").with_priority(Priority::Critical).priority(),
            Priority::Critical
        );
        assert_eq!(
            StatusItem::text("low").with_priority(Priority::Low).priority(),
            Priority::Low
        );
    }

    #[test]
    fn with_priority_chains_after_other_setters() {
        let item = StatusItem::key_hint("q", "Quit").with_priority(Priority::High);
        assert_eq!(item.priority(), Priority::High);
        assert_eq!(item.width(), 6);
        assert_eq!(item.render_to_string(), "q Quit");
    }

    // -----------------------------------------------------------------------
    // Collapse tests
    // -----------------------------------------------------------------------

    #[test]
    fn collapse_all_fit() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("A"),
            StatusItem::text("B"),
            StatusItem::text("C"),
        ];
        assert_eq!(sl.collapse_region(&items, 20).len(), 3);
    }

    #[test]
    fn collapse_removes_lowest_priority_first() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("HIGH_PRIO").with_priority(Priority::High),
            StatusItem::text("LOW_PRIO").with_priority(Priority::Low),
        ];
        let collapsed = sl.collapse_region(&items, 10);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].render_to_string(), "HIGH_PRIO");
    }

    #[test]
    fn collapse_never_removes_critical() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("CRITICAL").with_priority(Priority::Critical),
            StatusItem::text("NORMAL").with_priority(Priority::Normal),
        ];
        let collapsed = sl.collapse_region(&items, 5);
        assert!(collapsed.iter().any(|i| i.render_to_string() == "CRITICAL"));
    }

    #[test]
    fn collapse_never_removes_critical_under_extreme_pressure() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("KEEP").with_priority(Priority::Critical),
            StatusItem::text("A").with_priority(Priority::Normal),
            StatusItem::text("B").with_priority(Priority::Low),
        ];
        let collapsed = sl.collapse_region(&items, 0);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].render_to_string(), "KEEP");
    }

    #[test]
    fn collapse_reverse_addition_order_same_priority() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("alpha"),
            StatusItem::text("beta"),
            StatusItem::text("gamma"),
        ];
        let collapsed = sl.collapse_region(&items, 10);
        assert_eq!(collapsed.len(), 2);
        assert_eq!(collapsed[0].render_to_string(), "alpha");
        assert_eq!(collapsed[1].render_to_string(), "beta");
    }

    #[test]
    fn collapse_mixed_priorities() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("CRIT_A").with_priority(Priority::Critical),
            StatusItem::text("NORMAL").with_priority(Priority::Normal),
            StatusItem::text("lo").with_priority(Priority::Low),
            StatusItem::text("lower").with_priority(Priority::Low),
        ];
        let collapsed = sl.collapse_region(&items, 15);
        assert_eq!(collapsed.len(), 2);
        assert_eq!(collapsed[0].render_to_string(), "CRIT_A");
        assert_eq!(collapsed[1].render_to_string(), "NORMAL");
    }

    #[test]
    fn collapse_spacers_ignored() {
        let sl = StatusLine::new().separator(" | ");
        let items = vec![
            StatusItem::text("L").with_priority(Priority::Critical),
            StatusItem::spacer(),
            StatusItem::text("R").with_priority(Priority::Low),
        ];
        let collapsed = sl.collapse_region(&items, 1);
        assert_eq!(collapsed.len(), 2);
        assert!(matches!(collapsed[1].kind, StatusItemKind::Spacer));
    }

    #[test]
    fn collapse_empty_region() {
        assert!(StatusLine::new().collapse_region(&[], 10).is_empty());
    }

    #[test]
    fn collapse_zero_width_removes_all_non_critical() {
        let items = vec![
            StatusItem::text("KEEP").with_priority(Priority::Critical),
            StatusItem::text("removeme").with_priority(Priority::Low),
        ];
        let collapsed = StatusLine::new().collapse_region(&items, 0);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].render_to_string(), "KEEP");
    }

    #[test]
    fn collapse_high_before_normal() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("HIGH").with_priority(Priority::High),
            StatusItem::text("NORMAL").with_priority(Priority::Normal),
            StatusItem::text("LOW_").with_priority(Priority::Low),
        ];
        let collapsed = sl.collapse_region(&items, 10);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].render_to_string(), "HIGH");
    }

    #[test]
    fn collapse_all_critical_stays() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("X").with_priority(Priority::Critical),
            StatusItem::text("Y").with_priority(Priority::Critical),
        ];
        assert_eq!(sl.collapse_region(&items, 2).len(), 2);
    }

    #[test]
    fn collapse_removes_multiple_lowest_first() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("NORM").with_priority(Priority::Normal),
            StatusItem::text("NORM2").with_priority(Priority::Normal),
            StatusItem::text("LOW").with_priority(Priority::Low),
            StatusItem::text("LOW2").with_priority(Priority::Low),
        ];
        let collapsed = sl.collapse_region(&items, 8);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].render_to_string(), "NORM");
    }

    #[test]
    fn collapse_ordering_different_priorities() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("A").with_priority(Priority::Normal),
            StatusItem::text("B").with_priority(Priority::Low),
            StatusItem::text("C").with_priority(Priority::High),
            StatusItem::text("D").with_priority(Priority::Critical),
            StatusItem::text("E").with_priority(Priority::Low),
        ];
        let collapsed = sl.collapse_region(&items, 5);
        assert_eq!(collapsed.len(), 3);
        assert_eq!(collapsed[0].render_to_string(), "A");
        assert_eq!(collapsed[1].render_to_string(), "C");
        assert_eq!(collapsed[2].render_to_string(), "D");
    }

    #[test]
    fn collapse_spacer_only_region() {
        assert_eq!(StatusLine::new().collapse_region(&[StatusItem::spacer()], 0).len(), 1);
    }

    #[test]
    fn collapse_removes_item_adjacent_to_spacer() {
        let sl = StatusLine::new().separator(" ");
        let items = vec![
            StatusItem::text("A"),
            StatusItem::spacer(),
            StatusItem::text("B"),
        ];
        assert_eq!(sl.collapse_region(&items, 2).len(), 3);
    }
}
