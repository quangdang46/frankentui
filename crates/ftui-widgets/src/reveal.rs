#![forbid(unsafe_code)]

//! Collapsible fold widget with three-way toggle.
//!
//! A [`Reveal`] widget shows a summary line that is always visible, with a
//! content section whose display is controlled by a three-way [`Fold`] state:
//!
//! | State | Summary | Content Area |
//! |---|---|---|
//! | `Collapsed` | Visible (with ▶ icon) | One-line truncated preview text |
//! | `Expanded` | Visible (with ▼ icon) | Full content widget rendered |
//! | `Hidden` | Visible (with ○ icon) | Empty |
//!
//! Clicking the summary line cycles through Collapsed → Expanded → Hidden → Collapsed.
//!
//! # Example
//!
//! ```ignore
//! use ftui_widgets::reveal::{Fold, Reveal, RevealState};
//! use ftui_render::frame::HitId;
//!
//! let widget = Reveal::new()
//!     .summary("Details")
//!     .hit_id(HitId::new(1))
//!     .collapsed_text("Click to reveal details…");
//!
//! let mut state = RevealState::new();
//! state.toggle(); // → Expanded
//! state.toggle(); // → Hidden
//! widget.render(area, frame, &mut state);
//! ```

use crate::{StatefulWidget, Widget, clear_text_area, draw_text_span};
use ftui_core::geometry::Rect;
use ftui_render::frame::{Frame, HitId, HitRegion};
use ftui_style::Style;
use std::time::Duration;

/// How the foldable content section is displayed.
///
/// Three states form a cycle: `Collapsed → Expanded → Hidden → Collapsed`.
/// Use [`Fold::toggle`] to advance through the cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fold {
    /// Summary visible; content shown as a one-line truncated preview.
    #[default]
    Collapsed,
    /// Summary visible; full content widget rendered.
    Expanded,
    /// Summary visible; content area is empty.
    Hidden,
}

impl Fold {
    /// Cycle to the next state for interactive toggle: Collapsed → Expanded → Collapsed.
    /// Hidden state is preserved for programmatic use but skipped during UI toggle.
    pub fn toggle(&mut self) {
        *self = match self {
            Fold::Collapsed => Fold::Expanded,
            Fold::Expanded | Fold::Hidden => Fold::Collapsed,
        };
    }
}

/// Hit region constant for the summary line (click to toggle fold state).
pub const REVEAL_HIT_SUMMARY: HitRegion = HitRegion::Custom(1);

/// Mutable state for a [`Reveal`] widget.
///
/// Tracks the current fold state and the elapsed time since the state was
/// last changed. Call [`tick`](RevealState::tick) each frame to accumulate
/// time, and [`toggle`](RevealState::toggle) to cycle the state on user
/// interaction.
#[derive(Debug, Clone)]
pub struct RevealState {
    /// Current fold state.
    pub fold: Fold,
    /// Elapsed time since the current state was entered.
    elapsed: Duration,
}

impl Default for RevealState {
    fn default() -> Self {
        Self::new()
    }
}

impl RevealState {
    /// Create a new state, defaulting to [`Fold::Collapsed`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            fold: Fold::Collapsed,
            elapsed: Duration::ZERO,
        }
    }

    /// Cycle to the next fold state and reset the elapsed timer to zero.
    pub fn toggle(&mut self) {
        self.fold.toggle();
        self.elapsed = Duration::ZERO;
    }

    /// Advance the elapsed timer by `delta`.
    pub fn tick(&mut self, delta: Duration) {
        self.elapsed += delta;
    }

    /// Duration spent in the current fold state.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

/// A collapsible section widget with a three-way fold.
///
/// The summary line is always visible. Clicking it cycles through
/// `Collapsed`, `Expanded`, and `Hidden` states. An optional content
/// [`Widget`] is rendered only in the `Expanded` state.
///
/// # Builder Methods
///
/// Use the builder API to configure the widget before rendering:
///
/// ```ignore
/// Reveal::new()
///     .summary("Section Title")
///     .hit_id(HitId::new(1))
///     .collapsed_text("Click to expand…")
///     .show_duration(true)
///     .style(Style::new().bold())
///     .summary_style(Style::new().fg(my_color))
///     .content(content_widget)
/// ```
pub struct Reveal<'a> {
    /// Summary text (always visible).
    summary: Option<&'a str>,
    /// Optional content widget (rendered only in Expanded state).
    content: Option<Box<dyn Widget + 'a>>,
    /// Hit-id for mouse interaction on the summary row.
    hit_id: Option<HitId>,
    /// Preview text shown below the summary when collapsed.
    collapsed_text: Option<&'a str>,
    /// Whether to display the state-duration alongside the summary.
    show_duration: bool,
    /// Base style.
    style: Style,
    /// Style override for the summary text row.
    summary_style: Option<Style>,
    /// Style for the content / collapsed-preview area (background clearing).
    content_style: Option<Style>,
}

impl Default for Reveal<'_> {
    fn default() -> Self {
        Self {
            summary: None,
            content: None,
            hit_id: None,
            collapsed_text: None,
            show_duration: false,
            style: Style::default(),
            summary_style: None,
            content_style: None,
        }
    }
}

impl<'a> Reveal<'a> {
    /// Create a new reveal widget with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the summary text (always visible).
    #[must_use]
    pub fn summary(mut self, text: &'a str) -> Self {
        self.summary = Some(text);
        self
    }

    /// Set a boxed content widget (rendered when expanded).
    #[must_use]
    pub fn content(mut self, widget: Box<dyn Widget + 'a>) -> Self {
        self.content = Some(widget);
        self
    }

    /// Set the hit-id for mouse interaction on the summary row.
    #[must_use]
    pub fn hit_id(mut self, id: HitId) -> Self {
        self.hit_id = Some(id);
        self
    }

    /// Set the preview text shown when collapsed (truncated to one line).
    #[must_use]
    pub fn collapsed_text(mut self, text: &'a str) -> Self {
        self.collapsed_text = Some(text);
        self
    }

    /// Enable or disable the state-duration display on the summary line.
    #[must_use]
    pub fn show_duration(mut self, show: bool) -> Self {
        self.show_duration = show;
        self
    }

    /// Set the base style.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set a style override for the summary text row.
    #[must_use]
    pub fn summary_style(mut self, style: Style) -> Self {
        self.summary_style = Some(style);
        self
    }

    /// Set a style for the content / collapsed-preview area (background).
    #[must_use]
    pub fn content_style(mut self, style: Style) -> Self {
        self.content_style = Some(style);
        self
    }

    /// Resolve the style for the summary row.
    fn resolved_summary_style(&self) -> Style {
        self.summary_style.unwrap_or(self.style)
    }

    /// Resolve the style for the content/preview area.
    fn resolved_content_style(&self) -> Style {
        self.content_style.unwrap_or(self.style)
    }

    /// Build the summary display line including icon, text, and optional duration.
    fn format_summary(&self, state: &RevealState) -> String {
        let icon = match state.fold {
            Fold::Collapsed => "\u{25b6}", // ▶
            Fold::Expanded => "\u{25bc}",  // ▼
            Fold::Hidden => "\u{25cb}",    // ○
        };
        let summary = self.summary.unwrap_or("");
        if self.show_duration && state.elapsed > Duration::ZERO {
            format!("{} {}  ({})", icon, summary, format_duration_short(state.elapsed))
        } else {
            format!("{} {}", icon, summary)
        }
    }

    /// Render the summary row and register hit region.
    fn render_summary(&self, area: Rect, frame: &mut Frame, state: &RevealState) {
        let summary_row = Rect::new(area.x, area.y, area.width, 1);
        let style = self.resolved_summary_style();

        // Handle degradation
        if !frame.buffer.degradation.render_decorative() {
            clear_text_area(frame, summary_row, style);
            // Still draw summary text even at low degradation
            if let Some(text) = self.summary {
                draw_text_span(frame, area.x, area.y, text, style, area.right());
            }
        } else {
            clear_text_area(frame, summary_row, style);
            let text = self.format_summary(state);
            draw_text_span(frame, area.x, area.y, &text, style, area.right());
        }

        // Register hit for mouse interaction
        if let Some(id) = self.hit_id {
            frame.register_hit(summary_row, id, REVEAL_HIT_SUMMARY, 0);
        }
    }
}

impl StatefulWidget for Reveal<'_> {
    type State = RevealState;

    fn render(&self, area: Rect, frame: &mut Frame, state: &mut Self::State) {
        if area.is_empty() || area.height == 0 {
            return;
        }

        let deg = frame.buffer.degradation;

        // ── Summary row (always visible) ──────────────────────────
        self.render_summary(area, frame, state);

        // ── Content area (rows below summary) ─────────────────────
        if area.height < 2 {
            return;
        }

        let content_area = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
        let content_style = self.resolved_content_style();

        match state.fold {
            Fold::Collapsed => {
                // Show one-line truncated preview
                clear_text_area(frame, content_area, content_style);
                if let Some(preview) = self.collapsed_text {
                    draw_text_span(
                        frame,
                        content_area.x,
                        content_area.y,
                        preview,
                        content_style,
                        content_area.right(),
                    );
                } else if deg.render_decorative() {
                    let placeholder = if deg.use_unicode_borders() {
                        "\u{2026}" // …
                    } else {
                        "..."
                    };
                    draw_text_span(
                        frame,
                        content_area.x,
                        content_area.y,
                        placeholder,
                        content_style,
                        content_area.right(),
                    );
                }
            }
            Fold::Expanded => {
                // Render the full content widget
                clear_text_area(frame, content_area, content_style);

                if let Some(ref content) = self.content {
                    content.render(content_area, frame);
                }
            }
            Fold::Hidden => {
                // Clear the content area (show nothing)
                clear_text_area(frame, content_area, content_style);
            }
        }
    }
}

impl Widget for Reveal<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let mut state = RevealState::new();
        StatefulWidget::render(self, area, frame, &mut state);
    }

    fn is_essential(&self) -> bool {
        true
    }
}

/// Format a Duration into a compact string.
///
/// Examples: `12.3s`, `45s`, `2m5s`, `1h30m`, `500ms`.
fn format_duration_short(d: Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms == 0 {
        return "0s".to_string();
    }
    let total_secs = d.as_secs();
    if total_secs == 0 {
        return format!("{}ms", total_ms);
    }
    if total_secs < 60 {
        let tenths = (d.subsec_millis()) / 100;
        if tenths > 0 {
            return format!("{}.{}s", total_secs, tenths);
        }
        return format!("{}s", total_secs);
    }
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins < 60 {
        if secs == 0 {
            return format!("{}m", mins);
        }
        return format!("{}m{}s", mins, secs);
    }
    let hours = mins / 60;
    let mins_rem = mins % 60;
    if mins_rem == 0 {
        return format!("{}h", hours);
    }
    format!("{}h{}m", hours, mins_rem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::budget::DegradationLevel;
    use ftui_render::cell::PackedRgba;
    use ftui_render::grapheme_pool::GraphemePool;

    // ── Helpers ───────────────────────────────────────────────────

    struct Fill(char);

    impl Widget for Fill {
        fn render(&self, area: Rect, frame: &mut Frame) {
            for y in area.y..area.bottom() {
                for x in area.x..area.right() {
                    frame.buffer.set(x, y, ftui_render::cell::Cell::from_char(self.0));
                }
            }
        }
    }

    fn render_to_string(
        widget: &Reveal,
        state: &mut RevealState,
        width: u16,
        height: u16,
    ) -> Vec<String> {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(width, height, &mut pool);
        let area = Rect::new(0, 0, width, height);
        StatefulWidget::render(widget, area, &mut frame, state);

        let mut lines = Vec::new();
        for y in 0..height {
            let mut row = String::with_capacity(width as usize);
            for x in 0..width {
                let ch = frame
                    .buffer
                    .get(x, y)
                    .and_then(|c| c.content.as_char())
                    .unwrap_or(' ');
                row.push(ch);
            }
            lines.push(row);
        }
        lines
    }

    fn assert_summary_contains(lines: &[String], needle: &str) {
        assert!(
            lines[0].contains(needle),
            "summary line {:?} does not contain {:?}",
            lines[0],
            needle
        );
    }

    // ── Fold state machine tests ──────────────────────────────────

    #[test]
    fn fold_default_is_collapsed() {
        assert_eq!(Fold::default(), Fold::Collapsed);
    }

    #[test]
    fn fold_toggle_collapsed_to_expanded() {
        let mut f = Fold::Collapsed;
        f.toggle();
        assert_eq!(f, Fold::Expanded);
    }

    #[test]
    fn fold_toggle_expanded_to_hidden() {
        let mut f = Fold::Expanded;
        f.toggle();
        assert_eq!(f, Fold::Hidden);
    }

    #[test]
    fn fold_toggle_hidden_to_collapsed() {
        let mut f = Fold::Hidden;
        f.toggle();
        assert_eq!(f, Fold::Collapsed);
    }

    #[test]
    fn fold_toggle_full_cycle() {
        let mut f = Fold::Collapsed;
        f.toggle();
        assert_eq!(f, Fold::Expanded);
        f.toggle();
        assert_eq!(f, Fold::Hidden);
        f.toggle();
        assert_eq!(f, Fold::Collapsed);
    }

    #[test]
    fn fold_clone() {
        let a = Fold::Expanded;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn fold_debug() {
        let d = format!("{:?}", Fold::Collapsed);
        assert!(d.contains("Collapsed"));
    }

    #[test]
    fn fold_partial_eq_distinct() {
        assert_ne!(Fold::Collapsed, Fold::Expanded);
        assert_ne!(Fold::Collapsed, Fold::Hidden);
        assert_ne!(Fold::Expanded, Fold::Hidden);
    }

    // ── RevealState tests ─────────────────────────────────────────

    #[test]
    fn state_default_fold_is_collapsed() {
        let s = RevealState::default();
        assert_eq!(s.fold, Fold::Collapsed);
    }

    #[test]
    fn state_new_fold_is_collapsed() {
        let s = RevealState::new();
        assert_eq!(s.fold, Fold::Collapsed);
        assert_eq!(s.elapsed(), Duration::ZERO);
    }

    #[test]
    fn state_toggle_changes_fold() {
        let mut s = RevealState::new();
        s.toggle();
        assert_eq!(s.fold, Fold::Expanded);
        s.toggle();
        assert_eq!(s.fold, Fold::Hidden);
        s.toggle();
        assert_eq!(s.fold, Fold::Collapsed);
    }

    #[test]
    fn state_toggle_resets_elapsed() {
        let mut s = RevealState::new();
        s.tick(Duration::from_secs(10));
        assert_eq!(s.elapsed(), Duration::from_secs(10));
        s.toggle();
        assert_eq!(s.elapsed(), Duration::ZERO);
    }

    #[test]
    fn state_tick_advances_elapsed() {
        let mut s = RevealState::new();
        s.tick(Duration::from_secs(5));
        assert_eq!(s.elapsed(), Duration::from_secs(5));
        s.tick(Duration::from_secs(3));
        assert_eq!(s.elapsed(), Duration::from_secs(8));
    }

    #[test]
    fn state_clone() {
        let mut a = RevealState::new();
        a.tick(Duration::from_secs(42));
        let b = a.clone();
        assert_eq!(a.fold, b.fold);
        assert_eq!(a.elapsed(), b.elapsed());
    }

    // ── Format duration tests ─────────────────────────────────────

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration_short(Duration::ZERO), "0s");
    }

    #[test]
    fn format_duration_ms() {
        assert_eq!(format_duration_short(Duration::from_millis(500)), "500ms");
        assert_eq!(format_duration_short(Duration::from_millis(1)), "1ms");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration_short(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn format_duration_seconds_with_tenths() {
        assert_eq!(
            format_duration_short(Duration::from_millis(12300)),
            "12.3s"
        );
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration_short(Duration::from_secs(125)), "2m5s");
        assert_eq!(format_duration_short(Duration::from_secs(120)), "2m");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration_short(Duration::from_secs(3660)), "1h1m");
        assert_eq!(format_duration_short(Duration::from_secs(7200)), "2h");
    }

    // ── Widget rendering: basic ───────────────────────────────────

    #[test]
    fn render_zero_area_no_panic() {
        let w = Reveal::new().summary("test");
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        let mut state = RevealState::new();
        StatefulWidget::render(&w, Rect::new(0, 0, 0, 0), &mut frame, &mut state);
    }

    #[test]
    fn render_summary_in_collapsed() {
        let w = Reveal::new().summary("Section");
        let mut state = RevealState::new();
        // Default is Collapsed
        let lines = render_to_string(&w, &mut state, 20, 3);
        assert_summary_contains(&lines, "Section");
        assert_summary_contains(&lines, "\u{25b6}"); // ▶
    }

    #[test]
    fn render_summary_in_expanded() {
        let w = Reveal::new().summary("Section");
        let mut state = RevealState::new();
        state.toggle(); // → Expanded
        let lines = render_to_string(&w, &mut state, 20, 3);
        assert_summary_contains(&lines, "Section");
        assert_summary_contains(&lines, "\u{25bc}"); // ▼
    }

    #[test]
    fn render_summary_in_hidden() {
        let w = Reveal::new().summary("Section");
        let mut state = RevealState::new();
        state.toggle(); // → Expanded
        state.toggle(); // → Hidden
        let lines = render_to_string(&w, &mut state, 20, 3);
        assert_summary_contains(&lines, "Section");
        assert_summary_contains(&lines, "\u{25cb}"); // ○
    }

    #[test]
    fn render_content_only_in_expanded() {
        let content = Box::new(Fill('X'));
        let w = Reveal::new().summary("Sec").content(content);
        let mut state = RevealState::new();

        // Collapsed: no content
        let lines = render_to_string(&w, &mut state, 10, 5);
        assert_eq!(lines[1].trim(), ""); // collapsed area is empty (no collapsed_text set)

        // Expanded: content visible
        state.toggle();
        let lines = render_to_string(&w, &mut state, 10, 5);
        assert_eq!(lines[1], "XXXXXXXXXX");

        // Hidden: content hidden
        state.toggle();
        let lines = render_to_string(&w, &mut state, 10, 5);
        assert_eq!(lines[1].trim(), "");
    }

    #[test]
    fn render_collapsed_preview_text() {
        let w = Reveal::new()
            .summary("Sec")
            .collapsed_text("Expand to see details");
        let mut state = RevealState::new();

        let lines = render_to_string(&w, &mut state, 30, 3);
        assert_summary_contains(&lines, "Sec");
        assert!(lines[1].contains("Expand to see details"));
    }

    #[test]
    fn render_collapsed_shows_placeholder_when_no_preview() {
        let w = Reveal::new().summary("Sec");
        let mut state = RevealState::new();

        let lines = render_to_string(&w, &mut state, 10, 3);
        assert!(lines[1].contains('\u{2026}') || lines[1].contains("..."));
    }

    #[test]
    fn render_hidden_shows_only_summary() {
        let w = Reveal::new()
            .summary("Sec")
            .collapsed_text("preview");
        let mut state = RevealState::new();
        state.toggle(); // Expanded
        state.toggle(); // Hidden

        let lines = render_to_string(&w, &mut state, 30, 5);
        assert_summary_contains(&lines, "Sec");
        // Content rows should be empty (spaces)
        for row in &lines[1..] {
            assert_eq!(row.trim(), "", "Hidden state content row should be empty, got: {row:?}");
        }
    }

    #[test]
    fn render_content_area_height_respected() {
        let content = Box::new(Fill('C'));
        let w = Reveal::new().summary("S").content(content);
        let mut state = RevealState::new();
        state.toggle(); // Expanded

        let lines = render_to_string(&w, &mut state, 5, 4);
        assert_eq!(lines.len(), 4);
        // Row 0: summary
        assert_summary_contains(&lines, "S");
        // Rows 1-3: content fills available space
        assert_eq!(lines[1], "CCCCC");
        assert_eq!(lines[2], "CCCCC");
        assert_eq!(lines[3], "CCCCC");
    }

    #[test]
    fn render_single_row_shows_only_summary() {
        let content = Box::new(Fill('C'));
        let w = Reveal::new().summary("Hi").content(content);
        let mut state = RevealState::new();
        state.toggle(); // Expanded

        let lines = render_to_string(&w, &mut state, 10, 1);
        assert_eq!(lines.len(), 1);
        assert_summary_contains(&lines, "Hi");
    }

    // ── Mouse / hit test ──────────────────────────────────────────

    #[test]
    fn hit_region_registered_on_summary() {
        let w = Reveal::new()
            .summary("Click Me")
            .hit_id(HitId::new(7));
        let mut state = RevealState::new();

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 5, &mut pool);
        let area = Rect::new(0, 0, 20, 5);
        StatefulWidget::render(&w, area, &mut frame, &mut state);

        // Click on summary row should return the hit
        let hit = frame.hit_test(0, 0);
        assert_eq!(hit, Some((HitId::new(7), REVEAL_HIT_SUMMARY, 0)));

        let hit = frame.hit_test(10, 0);
        assert_eq!(hit, Some((HitId::new(7), REVEAL_HIT_SUMMARY, 0)));

        // Click on content row should NOT be a hit
        let hit = frame.hit_test(0, 1);
        assert_eq!(hit, None);
    }

    #[test]
    fn no_hit_region_when_hit_id_not_set() {
        let w = Reveal::new().summary("No Hit");
        let mut state = RevealState::new();

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 3, &mut pool);
        let area = Rect::new(0, 0, 20, 3);
        StatefulWidget::render(&w, area, &mut frame, &mut state);

        let hit = frame.hit_test(5, 0);
        assert_eq!(hit, None);
    }

    // ── Duration display ──────────────────────────────────────────

    #[test]
    fn duration_not_shown_when_disabled() {
        let w = Reveal::new()
            .summary("Sec")
            .show_duration(false);
        let mut state = RevealState::new();
        state.tick(Duration::from_secs(10));

        let lines = render_to_string(&w, &mut state, 30, 3);
        assert!(!lines[0].contains("10s"), "duration should not appear: {}", lines[0]);
    }

    #[test]
    fn duration_shown_when_enabled() {
        let w = Reveal::new()
            .summary("Sec")
            .show_duration(true);
        let mut state = RevealState::new();
        state.tick(Duration::from_secs(12));

        let lines = render_to_string(&w, &mut state, 30, 3);
        assert_summary_contains(&lines, "12s");
    }

    #[test]
    fn duration_not_shown_when_zero() {
        let w = Reveal::new()
            .summary("Sec")
            .show_duration(true);
        let mut state = RevealState::new();

        let lines = render_to_string(&w, &mut state, 30, 3);
        // No duration should be displayed when elapsed is zero
        // Summary should just be "▶ Sec"
        assert_eq!(lines[0].trim(), "\u{25b6} Sec");
    }

    // ── Style tests ───────────────────────────────────────────────

    #[test]
    fn summary_style_applied() {
        let fg = PackedRgba::rgb(200, 100, 0);
        let w = Reveal::new()
            .summary("Styled")
            .summary_style(Style::new().fg(fg));
        let mut state = RevealState::new();

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 3, &mut pool);
        let area = Rect::new(0, 0, 20, 3);
        StatefulWidget::render(&w, area, &mut frame, &mut state);

        let cell = frame.buffer.get(2, 0).unwrap(); // position of "S"
        assert_eq!(cell.fg, fg);
    }

    #[test]
    fn content_style_applied_to_preview_area() {
        let bg = PackedRgba::rgb(50, 50, 50);
        let w = Reveal::new()
            .summary("T")
            .collapsed_text("preview")
            .content_style(Style::new().bg(bg));
        let mut state = RevealState::new();

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 3, &mut pool);
        let area = Rect::new(0, 0, 20, 3);
        StatefulWidget::render(&w, area, &mut frame, &mut state);

        let cell = frame.buffer.get(0, 1).unwrap(); // first content cell
        assert_eq!(cell.bg, bg);
    }

    // ── Builder tests ─────────────────────────────────────────────

    #[test]
    fn builder_default() {
        let w = Reveal::new();
        assert!(w.summary.is_none());
        assert!(w.content.is_none());
        assert!(w.hit_id.is_none());
        assert!(!w.show_duration);
    }

    #[test]
    fn builder_summary() {
        let w = Reveal::new().summary("Hello");
        assert_eq!(w.summary, Some("Hello"));
    }

    #[test]
    fn builder_content_boxed() {
        let w = Reveal::new().content(Box::new(Fill('Z')));
        assert!(w.content.is_some());
    }

    #[test]
    fn builder_hit_id() {
        let w = Reveal::new().hit_id(HitId::new(42));
        assert_eq!(w.hit_id, Some(HitId::new(42)));
    }

    #[test]
    fn builder_collapsed_text() {
        let w = Reveal::new().collapsed_text("click here");
        assert_eq!(w.collapsed_text, Some("click here"));
    }

    #[test]
    fn builder_show_duration() {
        let w = Reveal::new().show_duration(true);
        assert!(w.show_duration);
    }

    #[test]
    fn builder_style() {
        let s = Style::new().bold();
        let w = Reveal::new().style(s);
        assert_eq!(w.style, s);
    }

    #[test]
    fn builder_summary_style() {
        let s = Style::new().italic();
        let w = Reveal::new().summary_style(s);
        assert_eq!(w.summary_style, Some(s));
    }

    #[test]
    fn builder_content_style() {
        let s = Style::new().dim();
        let w = Reveal::new().content_style(s);
        assert_eq!(w.content_style, Some(s));
    }

    // ── Degradation tests ─────────────────────────────────────────

    #[test]
    fn degradation_essential_still_shows_summary() {
        let w = Reveal::new().summary("Critical");
        let mut state = RevealState::new();

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 3, &mut pool);
        frame.buffer.degradation = DegradationLevel::EssentialOnly;
        let area = Rect::new(0, 0, 20, 3);
        StatefulWidget::render(&w, area, &mut frame, &mut state);

        // Summary should still be visible
        let cell = frame.buffer.get(2, 0).unwrap();
        assert_eq!(cell.content.as_char(), Some('C'));
    }

    #[test]
    fn degradation_no_styling_uses_default_style() {
        let fg = PackedRgba::rgb(200, 0, 0);
        let w = Reveal::new()
            .summary("T")
            .summary_style(Style::new().fg(fg));
        let mut state = RevealState::new();

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 3, &mut pool);
        frame.buffer.degradation = DegradationLevel::NoStyling;
        let area = Rect::new(0, 0, 20, 3);
        StatefulWidget::render(&w, area, &mut frame, &mut state);

        let cell = frame.buffer.get(2, 0).unwrap();
        assert_ne!(cell.fg, fg);
    }

    #[test]
    fn is_essential_true() {
        let w = Reveal::new();
        assert!(w.is_essential());
    }

    // ── Edge cases ────────────────────────────────────────────────

    #[test]
    fn no_summary_text_still_renders_icon() {
        let w = Reveal::new();
        let mut state = RevealState::new();

        let lines = render_to_string(&w, &mut state, 10, 3);
        assert_summary_contains(&lines, "\u{25b6}");
    }

    #[test]
    fn width_clips_long_summary() {
        let w = Reveal::new().summary("ABCDEFGHIJKLMNOPQ");
        let mut state = RevealState::new();

        let lines = render_to_string(&w, &mut state, 5, 3);
        // Should be exactly 5 chars wide
        assert_eq!(lines[0].len(), 5);
    }

    #[test]
    fn content_rendered_near_bottom_of_large_area() {
        let content = Box::new(Fill('Y'));
        let w = Reveal::new().summary("S").content(content);
        let mut state = RevealState::new();
        state.toggle(); // Expanded

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 10, &mut pool);
        let area = Rect::new(2, 3, 5, 6);
        StatefulWidget::render(&w, area, &mut frame, &mut state);

        // Summary at row 3
        let cell = frame.buffer.get(2, 3).unwrap();
        assert_eq!(cell.content.as_char(), Some('\u{25bc}'));

        // Content at rows 4-8
        for y in 4..9 {
            let cell = frame.buffer.get(2, y).unwrap();
            assert_eq!(cell.content.as_char(), Some('Y'));
        }

        // Row 9 should be untouched (outside area)
        let cell = frame.buffer.get(2, 9).unwrap();
        assert!(cell.is_empty());
    }

    #[test]
    fn stateless_render_uses_collapsed() {
        let w = Reveal::new().summary("Stateless");

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 3, &mut pool);
        let area = Rect::new(0, 0, 20, 3);
        Widget::render(&w, area, &mut frame);

        // Should render as collapsed (default state)
        let cell = frame.buffer.get(0, 0).unwrap();
        assert_eq!(cell.content.as_char(), Some('\u{25b6}'));
    }

    #[test]
    fn hit_constant_distinct_from_content() {
        use ftui_render::frame::HitRegion;
        assert_ne!(REVEAL_HIT_SUMMARY, HitRegion::Content);
        assert_ne!(REVEAL_HIT_SUMMARY, HitRegion::Button);
    }

    #[test]
    fn format_duration_very_small() {
        assert_eq!(format_duration_short(Duration::from_micros(1)), "0s");
    }
}
