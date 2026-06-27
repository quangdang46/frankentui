#![forbid(unsafe_code)]

//! Turn metrics "worked for" separator widget.
//!
//! A full-width horizontal rule with centered turn metrics,
//! showing duration, tool call count, and cost for a turn.
//!
//! # Example
//!
//! ```ignore
//! use ftui_widgets::turn_separator::{TurnMetrics, TurnSeparator};
//! use std::time::Duration;
//!
//! let metrics = TurnMetrics::new()
//!     .duration(Duration::from_secs_f64(12.3))
//!     .tool_calls(4)
//!     .cost(0.042);
//!
//! let sep = TurnSeparator::new().metrics(metrics);
//! sep.render(area, frame);
//! ```
//!
//! When in-progress, the separator shows " working… " (with ellipsis).
//! When metrics are empty, it renders a simple horizontal rule.

use crate::{Widget, apply_style, clear_text_row, draw_text_span};
use ftui_core::geometry::Rect;
use ftui_render::buffer::Buffer;
use ftui_render::cell::Cell;
use ftui_render::frame::Frame;
use ftui_style::Style;
use ftui_text::display_width;
use std::time::Duration;

/// Metrics for a single turn, displayed in a [`TurnSeparator`].
///
/// All fields are optional; only the set fields appear in the
/// formatted string. When no metrics are set and the turn is not
/// in-progress, the separator renders as a simple rule.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnMetrics {
    /// Wall-clock duration of the turn.
    pub duration: Option<Duration>,
    /// Number of tool calls made during this turn.
    pub tool_calls: Option<u64>,
    /// Monetary cost of the turn in dollars.
    pub cost: Option<f64>,
    /// Whether the turn is still in progress.
    pub in_progress: bool,
}

impl TurnMetrics {
    /// Create default (empty) metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the turn duration.
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Set the tool call count.
    pub fn tool_calls(mut self, n: u64) -> Self {
        self.tool_calls = Some(n);
        self
    }

    /// Set the turn cost in dollars.
    pub fn cost(mut self, c: f64) -> Self {
        self.cost = Some(c);
        self
    }

    /// Set whether the turn is in progress.
    pub fn in_progress(mut self, v: bool) -> Self {
        self.in_progress = v;
        self
    }

    /// Check if the metrics are empty (no data and not in-progress).
    pub fn is_empty(&self) -> bool {
        !self.in_progress
            && self.duration.is_none()
            && self.tool_calls.is_none()
            && self.cost.is_none()
    }

    /// Build the metrics display text.
    fn format_text(&self) -> Option<String> {
        if self.in_progress {
            return Some(" working\u{2026}".to_string());
        }
        if self.is_empty() {
            return None;
        }

        let mut parts: Vec<String> = Vec::new();

        if let Some(d) = self.duration {
            parts.push(format_duration_human(d));
        }
        if let Some(n) = self.tool_calls {
            parts.push(format!("{n} tool call{}", if n == 1 { "" } else { "s" }));
        }
        if let Some(c) = self.cost {
            if c >= 0.0 {
                let scaled = c * 1000.0;
                let tenths = (scaled + 0.05).floor() as u64;
                let dollars = tenths / 1000;
                let cents = (tenths % 1000) / 10;
                let mills = tenths % 10;
                if dollars > 0 {
                    if mills == 0 && cents == 0 {
                        parts.push(format!("${dollars}"));
                    } else if mills == 0 {
                        parts.push(format!("${dollars}.{cents:02}"));
                    } else {
                        parts.push(format!("${dollars}.{cents:02}{mills}"));
                    }
                } else if cents > 0 || mills > 0 {
                    if mills == 0 {
                        parts.push(format!("$0.{cents:02}"));
                    } else {
                        parts.push(format!("$0.{cents:02}{mills}"));
                    }
                } else {
                    parts.push("$0.00".to_string());
                }
            }
        }

        if parts.is_empty() {
            return None;
        }

        let joined = parts.join(" \u{b7} ");
        if self.duration.is_some() {
            Some(format!(" worked for {joined}"))
        } else {
            Some(format!(" {joined}"))
        }
    }
}

impl Default for TurnMetrics {
    fn default() -> Self {
        Self {
            duration: None,
            tool_calls: None,
            cost: None,
            in_progress: false,
        }
    }
}

/// Format a Duration into a concise human-readable string.
///
/// Examples: `12.3s`, `2m5s`, `1h30m15s`.
fn format_duration_human(d: Duration) -> String {
    let total_secs = d.as_secs();
    let subsec_nanos = d.subsec_nanos();

    if total_secs == 0 && subsec_nanos == 0 {
        return "0s".to_string();
    }

    // For sub-second durations, show in milliseconds
    if total_secs == 0 {
        let millis = d.as_millis();
        if millis >= 1 {
            return format!("{}ms", millis);
        }
        // Below 1ms, show fractional seconds
        return format!("{:.3}s", d.as_secs_f64());
    }

    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    // Show sub-second precision when there's a fractional part
    if hours == 0 && minutes == 0 && subsec_nanos > 0 {
        let tenths = subsec_nanos / 100_000_000;
        return format!("{}.{}s", seconds, tenths);
    }

    if hours > 0 {
        format!("{hours}h{minutes}m{seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// A full-width horizontal rule that optionally displays centered turn metrics.
///
/// Renders U+2500 box-drawing horizontal characters across the full width.
/// When metrics are provided, a padded metrics string is centered on the rule.
/// When in-progress, shows " working… " centered.
/// Empty metrics produce a simple full-width rule with no text.
pub struct TurnSeparator {
    /// Optional turn metrics to display.
    metrics: Option<TurnMetrics>,
    /// Style for the rule line.
    style: Style,
    /// Style for the metrics text (defaults to rule style).
    metrics_style: Option<Style>,
}

impl Default for TurnSeparator {
    fn default() -> Self {
        Self {
            metrics: None,
            style: Style::default().dim(),
            metrics_style: None,
        }
    }
}

impl TurnSeparator {
    /// Create a new turn separator with default settings.
    ///
    /// Default style is dimmed. No metrics text is rendered initially.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the turn metrics to display.
    pub fn metrics(mut self, metrics: TurnMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Set the style for the rule characters.
    ///
    /// The default style is dimmed, giving a subtle visual separation.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set a separate style for the metrics text.
    ///
    /// If not set, the separator's main style is used for metrics text as well.
    pub fn metrics_style(mut self, style: Style) -> Self {
        self.metrics_style = Some(style);
        self
    }

    /// Fill a range of cells on row `y` with the horizontal rule character.
    fn fill_rule_char(&self, buf: &mut Buffer, y: u16, start: u16, end: u16) {
        let ch = if buf.degradation.use_unicode_borders() {
            '\u{2500}'
        } else {
            '-'
        };
        let style = if buf.degradation.apply_styling() {
            self.style
        } else {
            Style::default()
        };
        for x in start..end {
            let mut cell = Cell::from_char(ch);
            apply_style(&mut cell, style);
            buf.set_fast(x, y, cell);
        }
    }

    /// Center and render the metrics text inside the rule.
    fn render_metrics(&self, frame: &mut Frame, area: Rect, text: &str) {
        let deg = frame.buffer.degradation;
        let y = area.y;
        let width = area.width;

        if width < 3 {
            self.fill_rule_char(&mut frame.buffer, y, area.x, area.right());
            return;
        }

        let text_width = display_width(text) as u16;
        let max_text_width = width.saturating_sub(2);
        let display_w = text_width.min(max_text_width);
        let text_block_width = display_w.saturating_add(2);

        let text_block_x =
            area.x.saturating_add((width.saturating_sub(text_block_width)) / 2);

        self.fill_rule_char(&mut frame.buffer, y, area.x, text_block_x);

        let deg_style = if deg.apply_styling() {
            self.style
        } else {
            Style::default()
        };
        let mut pad_l = Cell::from_char(' ');
        apply_style(&mut pad_l, deg_style);
        frame.buffer.set_fast(text_block_x, y, pad_l);

        let text_x = text_block_x.saturating_add(1);
        let text_end = text_x.saturating_add(display_w);
        let metrics_style = if deg.apply_styling() {
            self.metrics_style.unwrap_or(self.style)
        } else {
            Style::default()
        };
        draw_text_span(frame, text_x, y, text, metrics_style, text_end);

        let right_pad_x = text_end;
        if right_pad_x < area.right() {
            let mut pad_r = Cell::from_char(' ');
            apply_style(&mut pad_r, deg_style);
            frame.buffer.set_fast(right_pad_x, y, pad_r);
        }

        let right_rule_start = right_pad_x.saturating_add(1);
        self.fill_rule_char(&mut frame.buffer, y, right_rule_start, area.right());
    }
}

impl Widget for TurnSeparator {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() {
            return;
        }

        if !frame.buffer.degradation.render_decorative() {
            clear_text_row(
                frame,
                Rect::new(area.x, area.y, area.width, 1),
                Style::default(),
            );
            return;
        }

        match &self.metrics {
            None => {
                self.fill_rule_char(&mut frame.buffer, area.y, area.x, area.right());
            }
            Some(metrics) if metrics.is_empty() => {
                self.fill_rule_char(&mut frame.buffer, area.y, area.x, area.right());
            }
            Some(metrics) => {
                let text = match metrics.format_text() {
                    Some(t) => t,
                    None => {
                        self.fill_rule_char(&mut frame.buffer, area.y, area.x, area.right());
                        return;
                    }
                };
                self.render_metrics(frame, area, &text);
            }
        }
    }

    fn is_essential(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::budget::DegradationLevel;
    use ftui_render::cell::PackedRgba;
    use ftui_render::grapheme_pool::GraphemePool;

    fn row_chars(buf: &Buffer, y: u16, width: u16) -> Vec<char> {
        (0..width)
            .map(|x| {
                buf.get(x, y)
                    .and_then(|c| c.content.as_char())
                    .unwrap_or(' ')
            })
            .collect()
    }

    fn row_string(buf: &Buffer, y: u16, width: u16) -> String {
        row_chars(buf, y, width).into_iter().collect()
    }

    fn row_trimmed(buf: &Buffer, y: u16, width: u16) -> String {
        let s: String = row_chars(buf, y, width).into_iter().collect();
        s.trim_end().to_string()
    }

    // ── TurnMetrics tests ──────────────────────────────────────────

    #[test]
    fn metrics_default_is_empty() {
        let m = TurnMetrics::default();
        assert!(m.is_empty());
        assert_eq!(m.duration, None);
        assert!(!m.in_progress);
    }

    #[test]
    fn metrics_new_is_empty() {
        let m = TurnMetrics::new();
        assert!(m.is_empty());
    }

    #[test]
    fn metrics_builder_sets_duration() {
        let m = TurnMetrics::new().duration(Duration::from_secs(5));
        assert_eq!(m.duration, Some(Duration::from_secs(5)));
        assert!(!m.is_empty());
    }

    #[test]
    fn metrics_builder_sets_tool_calls() {
        let m = TurnMetrics::new().tool_calls(3);
        assert_eq!(m.tool_calls, Some(3));
        assert!(!m.is_empty());
    }

    #[test]
    fn metrics_builder_sets_cost() {
        let m = TurnMetrics::new().cost(1.23);
        assert_eq!(m.cost, Some(1.23));
        assert!(!m.is_empty());
    }

    #[test]
    fn metrics_in_progress_not_empty() {
        let m = TurnMetrics::new().in_progress(true);
        assert!(m.in_progress);
        assert!(!m.is_empty());
    }

    #[test]
    fn metrics_in_progress_false_is_empty_when_no_fields() {
        let m = TurnMetrics::new().in_progress(false);
        assert!(m.is_empty());
    }

    #[test]
    fn metrics_clone_eq() {
        let a = TurnMetrics::new()
            .duration(Duration::from_secs(10))
            .tool_calls(4)
            .cost(0.50);
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn metrics_debug_format() {
        let m = TurnMetrics::new().duration(Duration::from_secs(1));
        let debug = format!("{m:?}");
        assert!(debug.contains("duration"));
    }

    // ── format_text tests ──────────────────────────────────────────

    #[test]
    fn format_text_empty_metrics_is_none() {
        let m = TurnMetrics::new();
        assert!(m.format_text().is_none());
    }

    #[test]
    fn format_text_in_progress() {
        let m = TurnMetrics::new().in_progress(true);
        assert_eq!(m.format_text(), Some(" working\u{2026}".to_string()));
    }

    #[test]
    fn format_text_duration_only() {
        let m = TurnMetrics::new().duration(Duration::from_secs(12));
        let text = m.format_text().unwrap();
        assert!(text.contains("12s"));
    }

    #[test]
    fn format_text_tool_calls_only() {
        let m = TurnMetrics::new().tool_calls(4);
        let text = m.format_text().unwrap();
        assert!(text.contains("4 tool calls"));
    }

    #[test]
    fn format_text_single_tool_call() {
        let m = TurnMetrics::new().tool_calls(1);
        let text = m.format_text().unwrap();
        assert!(text.contains("1 tool call"));
        assert!(!text.contains("calls"));
    }

    #[test]
    fn format_text_cost_only() {
        let m = TurnMetrics::new().cost(0.042);
        let text = m.format_text().unwrap();
        assert!(text.contains("$0.04"));
    }

    #[test]
    fn format_text_all_fields() {
        let m = TurnMetrics::new()
            .duration(Duration::from_secs_f64(12.3))
            .tool_calls(4)
            .cost(0.042);
        let text = m.format_text().unwrap();
        assert!(text.contains("12.3s"), "got: {text}");
        assert!(text.contains("4 tool calls"), "got: {text}");
        assert!(text.contains("$0.04"), "got: {text}");
    }

    #[test]
    fn format_text_cost_zero() {
        let m = TurnMetrics::new().cost(0.0);
        let text = m.format_text().unwrap();
        assert!(text.contains("$0.00"));
    }

    #[test]
    fn format_text_cost_large() {
        let m = TurnMetrics::new().cost(123.45);
        let text = m.format_text().unwrap();
        assert!(text.contains("$123.45"));
    }

    #[test]
    fn format_text_separator_is_middle_dot() {
        let m = TurnMetrics::new()
            .duration(Duration::from_secs_f64(1.5))
            .tool_calls(2)
            .cost(0.10);
        let text = m.format_text().unwrap();
        assert!(text.contains(" \u{b7} "), "missing middle-dot separator");
    }

    #[test]
    fn format_text_in_progress_overrides_other_fields() {
        let metrics = TurnMetrics::new()
            .duration(Duration::from_secs(99))
            .tool_calls(10)
            .cost(5.0)
            .in_progress(true);
        let text = metrics.format_text().unwrap();
        assert_eq!(text, " working\u{2026}");
        assert!(!text.contains("99s"));
        assert!(!text.contains("10 tool calls"));
    }

    #[test]
    fn format_text_all_components_full_line() {
        let m = TurnMetrics::new()
            .duration(Duration::from_secs_f64(12.3))
            .tool_calls(4)
            .cost(0.042);
        let text = m.format_text().unwrap();
        assert_eq!(text, " worked for 12.3s \u{b7} 4 tool calls \u{b7} $0.042");
    }

    #[test]
    fn format_text_tool_calls_zero() {
        let m = TurnMetrics::new().tool_calls(0);
        let text = m.format_text().unwrap();
        assert!(text.contains("0 tool calls"));
    }

    #[test]
    fn format_text_with_tool_calls_and_cost_but_no_duration() {
        let m = TurnMetrics::new().tool_calls(3).cost(0.15);
        let text = m.format_text().unwrap();
        assert!(text.contains("3 tool calls"));
        assert!(text.contains("$0.15"));
        assert!(!text.contains("worked for "));
    }

    // ── format_duration_human tests ────────────────────────────────

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration_human(Duration::ZERO), "0s");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration_human(Duration::from_secs(42)), "42s");
    }

    #[test]
    fn format_duration_minutes_seconds() {
        assert_eq!(format_duration_human(Duration::from_secs(125)), "2m5s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration_human(Duration::from_secs(3665)), "1h1m5s");
    }

    #[test]
    fn format_duration_with_tenths() {
        let d = Duration::from_millis(12300);
        assert_eq!(format_duration_human(d), "12.3s");
    }

    #[test]
    fn format_duration_sub_second() {
        assert_eq!(format_duration_human(Duration::from_millis(500)), "500ms");
        assert_eq!(format_duration_human(Duration::from_millis(10)), "10ms");
    }

    #[test]
    fn format_duration_large_hours() {
        assert_eq!(
            format_duration_human(Duration::from_secs(100 * 3600 + 30 * 60 + 15)),
            "100h30m15s"
        );
    }

    #[test]
    fn format_duration_exact_hour() {
        assert_eq!(format_duration_human(Duration::from_secs(3600)), "1h0m0s");
    }

    #[test]
    fn format_duration_exact_minute() {
        assert_eq!(format_duration_human(Duration::from_secs(60)), "1m0s");
    }

    // ── TurnSeparator rendering tests ──────────────────────────────

    #[test]
    fn default_separator_renders_full_width() {
        let sep = TurnSeparator::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        sep.render(Rect::new(0, 0, 10, 1), &mut frame);

        let row = row_chars(&frame.buffer, 0, 10);
        assert!(row.iter().all(|&c| c == '\u{2500}'), "Expected all U+2500, got: {row:?}");
    }

    #[test]
    fn default_style_is_dimmed() {
        let sep = TurnSeparator::new();
        assert!(sep.style.attrs.map_or(false, |a| a.contains(ftui_style::StyleFlags::DIM)));
    }

    #[test]
    fn zero_area_no_panic() {
        let sep = TurnSeparator::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        sep.render(Rect::new(0, 0, 0, 0), &mut frame);
    }

    #[test]
    fn empty_metrics_is_plain_rule() {
        let sep = TurnSeparator::new().metrics(TurnMetrics::new());
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        sep.render(Rect::new(0, 0, 10, 1), &mut frame);

        let row = row_chars(&frame.buffer, 0, 10);
        assert!(row.iter().all(|&c| c == '\u{2500}'), "Empty metrics should be plain rule");
    }

    #[test]
    fn metrics_text_appears_centered() {
        let metrics = TurnMetrics::new().duration(Duration::from_secs_f64(5.0));
        let sep = TurnSeparator::new().metrics(metrics);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        sep.render(Rect::new(0, 0, 20, 1), &mut frame);

        let s = row_trimmed(&frame.buffer, 0, 20);
        assert!(s.contains("5s"), "Should contain duration, got: '{s}'");
        assert!(s.contains('\u{2500}'), "Should contain rule chars");
    }

    #[test]
    fn in_progress_text_renders() {
        let metrics = TurnMetrics::new().in_progress(true);
        let sep = TurnSeparator::new().metrics(metrics);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        sep.render(Rect::new(0, 0, 20, 1), &mut frame);

        let s = row_trimmed(&frame.buffer, 0, 20);
        assert!(s.contains("working"), "Should show working indicator");
        assert!(s.contains('\u{2500}'), "Should contain rule chars");
    }

    #[test]
    fn narrow_width_falls_back_to_plain_rule() {
        let metrics = TurnMetrics::new().duration(Duration::from_secs(99));
        let sep = TurnSeparator::new().metrics(metrics);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(2, 1, &mut pool);
        sep.render(Rect::new(0, 0, 2, 1), &mut frame);

        let row = row_chars(&frame.buffer, 0, 2);
        assert!(row.iter().all(|&c| c == '\u{2500}'), "Narrow should be plain rule");
    }

    #[test]
    fn style_applied_to_rule_chars() {
        let fg = PackedRgba::rgb(128, 128, 128);
        let sep = TurnSeparator::new().style(Style::new().fg(fg));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 1, &mut pool);
        sep.render(Rect::new(0, 0, 5, 1), &mut frame);

        for x in 0..5 {
            assert_eq!(frame.buffer.get(x, 0).unwrap().fg, fg);
        }
    }

    #[test]
    fn offset_area() {
        let sep = TurnSeparator::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 5, &mut pool);
        sep.render(Rect::new(5, 3, 8, 1), &mut frame);

        assert_ne!(frame.buffer.get(4, 3).unwrap().content.as_char(), Some('\u{2500}'));
        assert_eq!(frame.buffer.get(5, 3).unwrap().content.as_char(), Some('\u{2500}'));
        assert_eq!(frame.buffer.get(12, 3).unwrap().content.as_char(), Some('\u{2500}'));
        assert_ne!(frame.buffer.get(13, 3).unwrap().content.as_char(), Some('\u{2500}'));
    }

    #[test]
    fn degradation_essential_only_skips() {
        let sep = TurnSeparator::new().metrics(TurnMetrics::new().duration(Duration::from_secs(5)));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        frame.buffer.degradation = DegradationLevel::EssentialOnly;
        sep.render(Rect::new(0, 0, 10, 1), &mut frame);

        for x in 0..10 {
            assert_eq!(frame.buffer.get(x, 0).unwrap().content.as_char(), Some(' '));
        }
    }

    #[test]
    fn degradation_skeleton_skips() {
        let sep = TurnSeparator::new().metrics(TurnMetrics::new().duration(Duration::from_secs(5)));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        frame.buffer.degradation = DegradationLevel::Skeleton;
        sep.render(Rect::new(0, 0, 10, 1), &mut frame);

        for x in 0..10 {
            assert_eq!(frame.buffer.get(x, 0).unwrap().content.as_char(), Some(' '));
        }
    }

    #[test]
    fn degradation_simple_borders_uses_ascii() {
        let sep = TurnSeparator::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        frame.buffer.degradation = DegradationLevel::SimpleBorders;
        sep.render(Rect::new(0, 0, 10, 1), &mut frame);

        let row = row_chars(&frame.buffer, 0, 10);
        assert!(row.iter().all(|&c| c == '-'), "Expected ASCII '-', got: {row:?}");
    }

    #[test]
    fn degradation_full_uses_unicode() {
        let sep = TurnSeparator::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        frame.buffer.degradation = DegradationLevel::Full;
        sep.render(Rect::new(0, 0, 10, 1), &mut frame);

        let row = row_chars(&frame.buffer, 0, 10);
        assert!(row.iter().all(|&c| c == '\u{2500}'), "Expected U+2500, got: {row:?}");
    }

    #[test]
    fn degradation_no_styling_uses_default_style() {
        let fg = PackedRgba::rgb(200, 0, 0);
        let sep = TurnSeparator::new()
            .style(Style::new().fg(fg).bold())
            .metrics(TurnMetrics::new().duration(Duration::from_secs(5)));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        frame.buffer.degradation = DegradationLevel::NoStyling;
        sep.render(Rect::new(0, 0, 10, 1), &mut frame);

        let cell = frame.buffer.get(0, 0).unwrap();
        assert_ne!(cell.fg, fg);
    }

    #[test]
    fn metrics_style_distinct_from_rule_style() {
        let rule_fg = PackedRgba::rgb(100, 100, 100);
        let metrics_fg = PackedRgba::rgb(0, 200, 0);
        let sep = TurnSeparator::new()
            .style(Style::new().fg(rule_fg))
            .metrics_style(Style::new().fg(metrics_fg))
            .metrics(TurnMetrics::new().duration(Duration::from_secs(5)));
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        sep.render(Rect::new(0, 0, 20, 1), &mut frame);

        let mut found_rule = false;
        let mut found_metrics = false;
        for x in 0..20 {
            let cell = frame.buffer.get(x, 0).unwrap();
            if cell.fg == rule_fg && cell.content.as_char() == Some('\u{2500}') {
                found_rule = true;
            }
            if cell.fg == metrics_fg && cell.content.as_char() == Some('5') {
                found_metrics = true;
            }
        }
        assert!(found_rule, "Should have rule chars with rule_fg");
        assert!(found_metrics, "Should have metrics text with metrics_fg");
    }

    #[test]
    fn single_cell_width() {
        let sep = TurnSeparator::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        sep.render(Rect::new(0, 0, 1, 1), &mut frame);

        assert_eq!(frame.buffer.get(0, 0).unwrap().content.as_char(), Some('\u{2500}'));
    }

    #[test]
    fn separator_not_essential() {
        let sep = TurnSeparator::new();
        assert!(!sep.is_essential());
    }

    #[test]
    fn full_width_rule_stays_in_area_bounds() {
        let sep = TurnSeparator::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 1, &mut pool);
        sep.render(Rect::new(2, 0, 5, 1), &mut frame);

        assert!(frame.buffer.get(0, 0).unwrap().is_empty());
        assert!(frame.buffer.get(1, 0).unwrap().is_empty());
        for x in 2..7 {
            assert_eq!(frame.buffer.get(x, 0).unwrap().content.as_char(), Some('\u{2500}'));
        }
        assert!(frame.buffer.get(7, 0).unwrap().is_empty());
    }

    #[test]
    fn metrics_text_surrounded_by_rule_chars() {
        let metrics = TurnMetrics::new().duration(Duration::from_secs(3));
        let sep = TurnSeparator::new().metrics(metrics);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(25, 1, &mut pool);
        sep.render(Rect::new(0, 0, 25, 1), &mut frame);

        let s = row_string(&frame.buffer, 0, 25);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[0], '\u{2500}');
        assert!(chars.contains(&'3'));
    }

    #[test]
    fn metrics_text_centered_with_rules_on_both_sides() {
        let metrics = TurnMetrics::new().duration(Duration::from_secs(7));
        let sep = TurnSeparator::new().metrics(metrics);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 1, &mut pool);
        sep.render(Rect::new(0, 0, 20, 1), &mut frame);

        let s = row_string(&frame.buffer, 0, 20);
        let text_start = s.find('7').unwrap();
        assert!(text_start > 0, "Should have rule before text");
        assert!(s.as_bytes()[text_start - 1] as char == ' ');
    }

    #[test]
    fn default_turn_separator_has_no_metrics() {
        let sep = TurnSeparator::default();
        assert!(sep.metrics.is_none());
    }
}
