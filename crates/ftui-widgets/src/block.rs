#![forbid(unsafe_code)]

use crate::Widget;
use crate::borders::{BorderSet, BorderType, Borders};
use crate::measurable::{MeasurableWidget, SizeConstraints};
use crate::{apply_style, draw_text_span, set_style_area};
use ftui_core::geometry::{Rect, Sides, Size};
use ftui_render::buffer::Buffer;
use ftui_render::cell::Cell;
use ftui_render::frame::Frame;
use ftui_style::Style;
use ftui_text::{grapheme_width, graphemes};

/// A widget that draws a block with optional borders, title, and padding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block<'a> {
    borders: Borders,
    border_style: Style,
    border_style_left: Option<Style>,
    border_style_right: Option<Style>,
    border_style_top: Option<Style>,
    border_style_bottom: Option<Style>,
    border_type: BorderType,
    title: Option<&'a str>,
    title_alignment: Alignment,
    style: Style,
    padding: Sides,
}

/// Text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    /// Align text to the left.
    Left,
    /// Center text horizontally.
    Center,
    /// Align text to the right.
    Right,
}

impl<'a> Block<'a> {
    /// Create a new block with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a block with all borders enabled and default padding.
    #[must_use]
    pub fn bordered() -> Self {
        Self::default().borders(Borders::ALL).padding(Sides::all(1))
    }

    /// Set which borders to render.
    #[must_use]
    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
        self
    }

    /// Set the style applied to border characters.
    #[must_use]
    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Set the style for the left border edge. Overrides `border_style` when set.
    #[must_use]
    pub fn border_style_left(mut self, style: Style) -> Self {
        self.border_style_left = Some(style);
        self
    }

    /// Set the style for the right border edge. Overrides `border_style` when set.
    #[must_use]
    pub fn border_style_right(mut self, style: Style) -> Self {
        self.border_style_right = Some(style);
        self
    }

    /// Set the style for the top border edge. Overrides `border_style` when set.
    #[must_use]
    pub fn border_style_top(mut self, style: Style) -> Self {
        self.border_style_top = Some(style);
        self
    }

    /// Set the style for the bottom border edge. Overrides `border_style` when set.
    #[must_use]
    pub fn border_style_bottom(mut self, style: Style) -> Self {
        self.border_style_bottom = Some(style);
        self
    }

    /// Set the border character set (e.g. square, rounded, double).
    #[must_use]
    pub fn border_type(mut self, border_type: BorderType) -> Self {
        self.border_type = border_type;
        self
    }

    /// Set the padding inside the borders.
    #[must_use]
    pub fn padding(mut self, padding: impl Into<Sides>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Get the border set for this block.
    pub(crate) fn border_set(&self) -> BorderSet {
        self.border_type.to_border_set()
    }

    /// Set the block title displayed on the top border.
    #[must_use]
    pub fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    /// Return the title text, if any.
    #[must_use]
    pub fn title_text(&self) -> Option<&str> {
        self.title
    }

    /// Set the horizontal alignment of the title.
    #[must_use]
    pub fn title_alignment(mut self, alignment: Alignment) -> Self {
        self.title_alignment = alignment;
        self
    }

    /// Set the background style for the entire block area.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Compute the inner area inside the block's borders and padding.
    #[must_use]
    pub fn inner(&self, area: Rect) -> Rect {
        let mut inner = area;

        if self.borders.contains(Borders::LEFT) {
            inner.x = inner.x.saturating_add(1);
            inner.width = inner.width.saturating_sub(1);
        }
        if self.borders.contains(Borders::TOP) {
            inner.y = inner.y.saturating_add(1);
            inner.height = inner.height.saturating_sub(1);
        }
        if self.borders.contains(Borders::RIGHT) {
            inner.width = inner.width.saturating_sub(1);
        }
        if self.borders.contains(Borders::BOTTOM) {
            inner.height = inner.height.saturating_sub(1);
        }

        inner.inner(self.padding)
    }

    /// Calculate the chrome (border + padding) size consumed by this block.
    ///
    /// Returns `(horizontal_chrome, vertical_chrome)` representing the
    /// total width and height consumed by borders and padding.
    #[must_use]
    pub fn chrome_size(&self) -> (u16, u16) {
        let border_h = self.borders.contains(Borders::LEFT) as u16
            + self.borders.contains(Borders::RIGHT) as u16;
        let border_v = self.borders.contains(Borders::TOP) as u16
            + self.borders.contains(Borders::BOTTOM) as u16;

        let padding_h = self.padding.left + self.padding.right;
        let padding_v = self.padding.top + self.padding.bottom;

        (
            border_h.saturating_add(padding_h),
            border_v.saturating_add(padding_v),
        )
    }

    /// Create a styled border cell.
    fn border_cell(&self, c: char, style: Style) -> Cell {
        let mut cell = Cell::from_char(c);
        apply_style(&mut cell, style);
        cell
    }

    /// Resolve the effective per-side styles by merging per-side overrides
    /// on top of the base style.
    ///
    /// When `degrade` is true, per-side overrides are ignored so that
    /// degradation (e.g. NoStyling) is not bypassed by side-specific styles.
    fn side_style(&self, base: Style, degrade: bool) -> (Style, Style, Style, Style) {
        if degrade {
            return (base, base, base, base);
        }
        let left = self.border_style_left.map_or(base, |s| s.merge(&base));
        let right = self.border_style_right.map_or(base, |s| s.merge(&base));
        let top = self.border_style_top.map_or(base, |s| s.merge(&base));
        let bottom = self.border_style_bottom.map_or(base, |s| s.merge(&base));
        (left, right, top, bottom)
    }

    fn render_borders(&self, area: Rect, buf: &mut Buffer, style: Style, degrade_overrides: bool) {
        if area.is_empty() {
            return;
        }

        let set = self.border_set();
        let (l_style, r_style, t_style, b_style) = self.side_style(style, degrade_overrides);

        // Edges
        if self.borders.contains(Borders::LEFT) {
            for y in area.y..area.bottom() {
                buf.set_fast(area.x, y, self.border_cell(set.vertical, l_style));
            }
        }
        if self.borders.contains(Borders::RIGHT) {
            let x = area.right() - 1;
            for y in area.y..area.bottom() {
                buf.set_fast(x, y, self.border_cell(set.vertical, r_style));
            }
        }
        if self.borders.contains(Borders::TOP) {
            for x in area.x..area.right() {
                buf.set_fast(x, area.y, self.border_cell(set.horizontal, t_style));
            }
        }
        if self.borders.contains(Borders::BOTTOM) {
            let y = area.bottom() - 1;
            for x in area.x..area.right() {
                buf.set_fast(x, y, self.border_cell(set.horizontal, b_style));
            }
        }

        // Corners (drawn after edges to overwrite edge characters at corners).
        // Left-side corners use the left style, right-side corners use the right style.
        if self.borders.contains(Borders::LEFT | Borders::TOP) {
            buf.set_fast(area.x, area.y, self.border_cell(set.top_left, l_style));
        }
        if self.borders.contains(Borders::RIGHT | Borders::TOP) {
            buf.set_fast(
                area.right() - 1,
                area.y,
                self.border_cell(set.top_right, r_style),
            );
        }
        if self.borders.contains(Borders::LEFT | Borders::BOTTOM) {
            buf.set_fast(
                area.x,
                area.bottom() - 1,
                self.border_cell(set.bottom_left, l_style),
            );
        }
        if self.borders.contains(Borders::RIGHT | Borders::BOTTOM) {
            buf.set_fast(
                area.right() - 1,
                area.bottom() - 1,
                self.border_cell(set.bottom_right, r_style),
            );
        }
    }

    /// Render borders using ASCII characters regardless of configured border_type.
    fn render_borders_ascii(&self, area: Rect, buf: &mut Buffer, style: Style, degrade_overrides: bool) {
        if area.is_empty() {
            return;
        }

        let set = crate::borders::BorderSet::ASCII;
        let (l_style, r_style, t_style, b_style) = self.side_style(style, degrade_overrides);

        if self.borders.contains(Borders::LEFT) {
            for y in area.y..area.bottom() {
                buf.set_fast(area.x, y, self.border_cell(set.vertical, l_style));
            }
        }
        if self.borders.contains(Borders::RIGHT) {
            let x = area.right() - 1;
            for y in area.y..area.bottom() {
                buf.set_fast(x, y, self.border_cell(set.vertical, r_style));
            }
        }
        if self.borders.contains(Borders::TOP) {
            for x in area.x..area.right() {
                buf.set_fast(x, area.y, self.border_cell(set.horizontal, t_style));
            }
        }
        if self.borders.contains(Borders::BOTTOM) {
            let y = area.bottom() - 1;
            for x in area.x..area.right() {
                buf.set_fast(x, y, self.border_cell(set.horizontal, b_style));
            }
        }

        if self.borders.contains(Borders::LEFT | Borders::TOP) {
            buf.set_fast(area.x, area.y, self.border_cell(set.top_left, l_style));
        }
        if self.borders.contains(Borders::RIGHT | Borders::TOP) {
            buf.set_fast(
                area.right() - 1,
                area.y,
                self.border_cell(set.top_right, r_style),
            );
        }
        if self.borders.contains(Borders::LEFT | Borders::BOTTOM) {
            buf.set_fast(
                area.x,
                area.bottom() - 1,
                self.border_cell(set.bottom_left, l_style),
            );
        }
        if self.borders.contains(Borders::RIGHT | Borders::BOTTOM) {
            buf.set_fast(
                area.right() - 1,
                area.bottom() - 1,
                self.border_cell(set.bottom_right, r_style),
            );
        }
    }

    fn render_title(&self, area: Rect, frame: &mut Frame) {
        if let Some(title) = self.title {
            if !self.borders.contains(Borders::TOP) || area.width < 3 {
                return;
            }

            let available_width = area.width.saturating_sub(2) as usize;
            if available_width == 0 {
                return;
            }

            let display_width = fitted_text_width(title, available_width);
            if display_width == 0 {
                return;
            }

            let x = match self.title_alignment {
                Alignment::Left => area.x.saturating_add(1),
                Alignment::Center => area
                    .x
                    .saturating_add(1)
                    .saturating_add(((available_width.saturating_sub(display_width)) / 2) as u16),
                Alignment::Right => area
                    .right()
                    .saturating_sub(1)
                    .saturating_sub(display_width as u16),
            };

            let max_x = area.right().saturating_sub(1);
            // Title border uses the top override style when set, otherwise falls back to
            // the global border_style.
            let title_style = self.border_style_top.unwrap_or(self.border_style);
            draw_text_span(frame, x, area.y, title, title_style, max_x);
        }
    }
}

impl Widget for Block<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "widget_render",
            widget = "Block",
            x = area.x,
            y = area.y,
            w = area.width,
            h = area.height
        )
        .entered();

        if area.is_empty() {
            return;
        }

        let deg = frame.degradation;
        let border_style = if deg.apply_styling() {
            self.border_style
        } else {
            Style::default()
        };

        // When styling is not applied, also clear the per-side overrides so
        // side_style() does not re-inject them.
        let degrade_override = !deg.apply_styling();

        // Skeleton+: skip everything, just clear area
        if !deg.render_content() {
            frame.buffer.fill(area, Cell::default());
            return;
        }

        // EssentialOnly: block chrome is purely decorative, so clear the owned
        // area instead of leaving stale borders/title content behind.
        if !deg.render_decorative() {
            frame.buffer.fill(area, Cell::default());
            if deg.apply_styling() {
                set_style_area(&mut frame.buffer, area, self.style);
            }
            return;
        }

        // Apply background/style
        if deg.apply_styling() {
            set_style_area(&mut frame.buffer, area, self.style);
        }

        // Render borders (with possible ASCII downgrade).
        // Per-side style overrides are also suppressed when styling is degraded.
        if deg.use_unicode_borders() {
            self.render_borders(area, &mut frame.buffer, border_style, degrade_override);
        } else {
            // Force ASCII borders regardless of configured border_type
            self.render_borders_ascii(area, &mut frame.buffer, border_style, degrade_override);
        }

        // Render title (skip at NoStyling to save time)
        if deg.apply_styling() {
            self.render_title(area, frame);
        } else if deg.render_decorative() {
            // Still show title but without styling
            // Pass frame to reuse draw_text_span
            if let Some(title) = self.title
                && self.borders.contains(Borders::TOP)
                && area.width >= 3
            {
                let available_width = area.width.saturating_sub(2) as usize;
                if available_width > 0 {
                    let display_width = fitted_text_width(title, available_width);
                    if display_width == 0 {
                        return;
                    }
                    let x = match self.title_alignment {
                        Alignment::Left => area.x.saturating_add(1),
                        Alignment::Center => area.x.saturating_add(1).saturating_add(
                            ((available_width.saturating_sub(display_width)) / 2) as u16,
                        ),
                        Alignment::Right => area
                            .right()
                            .saturating_sub(1)
                            .saturating_sub(display_width as u16),
                    };
                    let max_x = area.right().saturating_sub(1);
                    // draw_text_span() preserves existing cell styles. Clear the
                    // title span first so NoStyling truly drops border styling.
                    frame.buffer.fill(
                        Rect::new(x, area.y, display_width as u16, 1),
                        Cell::default(),
                    );
                    draw_text_span(frame, x, area.y, title, Style::default(), max_x);
                }
            }
        }
    }
}

impl MeasurableWidget for Block<'_> {
    fn measure(&self, _available: Size) -> SizeConstraints {
        let (chrome_width, chrome_height) = self.chrome_size();
        let chrome = Size::new(chrome_width, chrome_height);

        // Block's intrinsic size is just its chrome (borders).
        // The minimum is the chrome size - less than this and borders overlap.
        // Preferred is also the chrome size - any inner content adds to this.
        // Maximum is unbounded - block can fill available space.
        SizeConstraints::at_least(chrome, chrome)
    }

    fn has_intrinsic_size(&self) -> bool {
        // Block has intrinsic size only if it has borders
        self.borders != Borders::empty()
    }
}

fn fitted_text_width(text: &str, max_width: usize) -> usize {
    let mut width = 0usize;
    for grapheme in graphemes(text) {
        let w = grapheme_width(grapheme);
        if w == 0 {
            continue;
        }
        if width.saturating_add(w) > max_width {
            break;
        }
        width += w;
    }
    width
}

// ============================================================================
// Accessibility
// ============================================================================

impl ftui_a11y::Accessible for Block<'_> {
    fn accessibility_nodes(&self, area: Rect) -> Vec<ftui_a11y::node::A11yNodeInfo> {
        use ftui_a11y::node::{A11yNodeInfo, A11yRole};

        let id = crate::a11y_node_id(area);
        let mut node = A11yNodeInfo::new(id, A11yRole::Group, area);
        if let Some(title) = self.title_text() {
            node = node.with_name(title);
        }
        vec![node]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::cell::PackedRgba;
    use ftui_render::grapheme_pool::GraphemePool;

    #[test]
    fn inner_with_all_borders() {
        let block = Block::new().borders(Borders::ALL);
        let area = Rect::new(0, 0, 10, 10);
        let inner = block.inner(area);
        assert_eq!(inner, Rect::new(1, 1, 8, 8));
    }

    #[test]
    fn inner_with_no_borders() {
        let block = Block::new();
        let area = Rect::new(0, 0, 10, 10);
        let inner = block.inner(area);
        assert_eq!(inner, area);
    }

    #[test]
    fn inner_with_partial_borders() {
        let block = Block::new().borders(Borders::TOP | Borders::LEFT);
        let area = Rect::new(0, 0, 10, 10);
        let inner = block.inner(area);
        assert_eq!(inner, Rect::new(1, 1, 9, 9));
    }

    #[test]
    fn render_empty_area() {
        let block = Block::new().borders(Borders::ALL);
        let area = Rect::new(0, 0, 0, 0);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        block.render(area, &mut frame);
    }

    #[test]
    fn render_block_with_square_borders() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square);
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('┌'));
        assert_eq!(buf.get(4, 0).unwrap().content.as_char(), Some('┐'));
        assert_eq!(buf.get(0, 2).unwrap().content.as_char(), Some('└'));
        assert_eq!(buf.get(4, 2).unwrap().content.as_char(), Some('┘'));
        assert_eq!(buf.get(2, 0).unwrap().content.as_char(), Some('─'));
        assert_eq!(buf.get(0, 1).unwrap().content.as_char(), Some('│'));
    }

    #[test]
    fn render_block_with_title() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .title("Hi");
        let area = Rect::new(0, 0, 10, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(1, 0).unwrap().content.as_char(), Some('H'));
        assert_eq!(buf.get(2, 0).unwrap().content.as_char(), Some('i'));
    }

    #[test]
    fn render_title_overrides_on_multiple_calls() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .title("First")
            .title("Second");
        let area = Rect::new(0, 0, 12, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(12, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(1, 0).unwrap().content.as_char(), Some('S'));
    }

    #[test]
    fn render_block_with_background() {
        let block = Block::new().style(Style::new().bg(PackedRgba::rgb(10, 20, 30)));
        let area = Rect::new(0, 0, 3, 2);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(3, 2, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().bg, PackedRgba::rgb(10, 20, 30));
        assert_eq!(buf.get(2, 1).unwrap().bg, PackedRgba::rgb(10, 20, 30));
    }

    #[test]
    fn inner_with_only_bottom() {
        let block = Block::new().borders(Borders::BOTTOM);
        let area = Rect::new(0, 0, 10, 10);
        let inner = block.inner(area);
        assert_eq!(inner, Rect::new(0, 0, 10, 9));
    }

    #[test]
    fn inner_with_only_right() {
        let block = Block::new().borders(Borders::RIGHT);
        let area = Rect::new(0, 0, 10, 10);
        let inner = block.inner(area);
        assert_eq!(inner, Rect::new(0, 0, 9, 10));
    }

    #[test]
    fn inner_saturates_on_tiny_area() {
        let block = Block::new().borders(Borders::ALL);
        let area = Rect::new(0, 0, 1, 1);
        let inner = block.inner(area);
        // 1x1 with all borders: x+1=1, w-2=0, y+1=1, h-2=0
        assert_eq!(inner.width, 0);
    }

    #[test]
    fn bordered_constructor() {
        let block = Block::bordered();
        assert_eq!(block.borders, Borders::ALL);
    }

    #[test]
    fn default_has_no_borders() {
        let block = Block::new();
        assert_eq!(block.borders, Borders::empty());
        assert!(block.title.is_none());
    }

    #[test]
    fn render_rounded_borders() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('╭'));
        assert_eq!(buf.get(4, 0).unwrap().content.as_char(), Some('╮'));
        assert_eq!(buf.get(0, 2).unwrap().content.as_char(), Some('╰'));
        assert_eq!(buf.get(4, 2).unwrap().content.as_char(), Some('╯'));
    }

    #[test]
    fn render_double_borders() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('╔'));
        assert_eq!(buf.get(4, 0).unwrap().content.as_char(), Some('╗'));
    }

    #[test]
    fn render_partial_borders_corners_only_when_edges_enabled() {
        let block = Block::new()
            .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
            .border_type(BorderType::Square);
        let area = Rect::new(0, 0, 4, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(4, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('┌'));
        assert_eq!(buf.get(0, 2).unwrap().content.as_char(), Some('└'));
        assert_eq!(buf.get(3, 0).unwrap().content.as_char(), Some('─'));
        assert_eq!(buf.get(3, 2).unwrap().content.as_char(), Some('─'));
        assert!(
            buf.get(3, 1).unwrap().is_empty()
                || buf.get(3, 1).unwrap().content.as_char() == Some(' ')
        );
    }

    #[test]
    fn render_vertical_only_borders_use_vertical_glyphs() {
        let block = Block::new()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_type(BorderType::Double);
        let area = Rect::new(0, 0, 4, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(4, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('║'));
        assert_eq!(buf.get(3, 0).unwrap().content.as_char(), Some('║'));
        assert!(
            buf.get(1, 0).unwrap().is_empty()
                || buf.get(1, 0).unwrap().content.as_char() == Some(' ')
        );
    }

    #[test]
    fn render_missing_left_keeps_horizontal_corner_logic() {
        let block = Block::new()
            .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
            .border_type(BorderType::Square);
        let area = Rect::new(0, 0, 4, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(4, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('─'));
        assert_eq!(buf.get(3, 0).unwrap().content.as_char(), Some('┐'));
        assert_eq!(buf.get(0, 2).unwrap().content.as_char(), Some('─'));
        assert_eq!(buf.get(3, 2).unwrap().content.as_char(), Some('┘'));
        assert_eq!(buf.get(3, 1).unwrap().content.as_char(), Some('│'));
    }

    #[test]
    fn render_title_left_aligned() {
        let block = Block::new()
            .borders(Borders::ALL)
            .title("Test")
            .title_alignment(Alignment::Left);
        let area = Rect::new(0, 0, 10, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(1, 0).unwrap().content.as_char(), Some('T'));
        assert_eq!(buf.get(2, 0).unwrap().content.as_char(), Some('e'));
    }

    #[test]
    fn render_title_center_aligned() {
        let block = Block::new()
            .borders(Borders::ALL)
            .title("Hi")
            .title_alignment(Alignment::Center);
        let area = Rect::new(0, 0, 10, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 3, &mut pool);
        block.render(area, &mut frame);

        // Title "Hi" (2 chars) in 8 available (10-2 borders), centered at offset 3
        let buf = &frame.buffer;
        assert_eq!(buf.get(4, 0).unwrap().content.as_char(), Some('H'));
        assert_eq!(buf.get(5, 0).unwrap().content.as_char(), Some('i'));
    }

    #[test]
    fn render_title_center_aligned_with_wide_grapheme() {
        let block = Block::new()
            .borders(Borders::ALL)
            .title("界")
            .title_alignment(Alignment::Center);
        let area = Rect::new(0, 0, 8, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(8, 3, &mut pool);
        block.render(area, &mut frame);

        // Available width = 6, title width = 2 => center offset 2 => x = 3
        let buf = &frame.buffer;
        let cell = buf.get(3, 0).unwrap();
        assert!(
            cell.content.as_char() == Some('界') || cell.content.is_grapheme(),
            "expected title grapheme at x=3"
        );
        assert!(buf.get(4, 0).unwrap().is_continuation());
    }

    #[test]
    fn render_title_right_aligned() {
        let block = Block::new()
            .borders(Borders::ALL)
            .title("Hi")
            .title_alignment(Alignment::Right);
        let area = Rect::new(0, 0, 10, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        // "Hi" right-aligned: right()-1 - 2 = col 7
        assert_eq!(buf.get(7, 0).unwrap().content.as_char(), Some('H'));
        assert_eq!(buf.get(8, 0).unwrap().content.as_char(), Some('i'));
    }

    #[test]
    fn render_title_right_aligned_truncated_wide_grapheme_uses_fitted_width() {
        let block = Block::new()
            .borders(Borders::ALL)
            .title("界界")
            .title_alignment(Alignment::Right);
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        let cell = buf.get(2, 0).unwrap();
        assert!(
            cell.content.as_char() == Some('界') || cell.content.is_grapheme(),
            "expected fitted wide title to be right aligned"
        );
        assert!(buf.get(3, 0).unwrap().is_continuation());
        assert_eq!(buf.get(1, 0).unwrap().content.as_char(), Some('─'));
    }

    #[test]
    fn render_multi_title_alignment_uses_last_title_and_alignment() {
        let block = Block::new()
            .borders(Borders::ALL)
            .title("Left")
            .title_alignment(Alignment::Left)
            .title("Right")
            .title_alignment(Alignment::Right);
        let area = Rect::new(0, 0, 12, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(12, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(6, 0).unwrap().content.as_char(), Some('R'));
        assert_ne!(buf.get(1, 0).unwrap().content.as_char(), Some('L'));
    }

    #[test]
    fn title_not_rendered_without_top_border() {
        let block = Block::new()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .title("Hi");
        let area = Rect::new(0, 0, 10, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        // No title should appear on row 0
        assert_ne!(buf.get(1, 0).unwrap().content.as_char(), Some('H'));
    }

    #[test]
    fn border_style_applied() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(PackedRgba::rgb(255, 0, 0)));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().fg, PackedRgba::rgb(255, 0, 0));
    }

    #[test]
    fn only_horizontal_borders() {
        let block = Block::new()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Square);
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        // Top and bottom should have horizontal lines
        assert_eq!(buf.get(2, 0).unwrap().content.as_char(), Some('─'));
        assert_eq!(buf.get(2, 2).unwrap().content.as_char(), Some('─'));
        // Left edge should be empty (no vertical border)
        assert!(
            buf.get(0, 1).unwrap().is_empty()
                || buf.get(0, 1).unwrap().content.as_char() == Some(' ')
        );
    }

    #[test]
    fn degradation_simple_borders_forces_ascii() {
        use ftui_render::budget::DegradationLevel;

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        frame.set_degradation(DegradationLevel::SimpleBorders);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('+'));
        assert_eq!(buf.get(4, 0).unwrap().content.as_char(), Some('+'));
        assert_eq!(buf.get(2, 0).unwrap().content.as_char(), Some('-'));
        assert_eq!(buf.get(0, 1).unwrap().content.as_char(), Some('|'));
    }

    #[test]
    fn degradation_simple_borders_partial_edges_use_ascii_corners() {
        use ftui_render::budget::DegradationLevel;

        let block = Block::new()
            .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
            .border_type(BorderType::Double);
        let area = Rect::new(0, 0, 4, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(4, 3, &mut pool);
        frame.set_degradation(DegradationLevel::SimpleBorders);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('-'));
        assert_eq!(buf.get(3, 0).unwrap().content.as_char(), Some('+'));
        assert_eq!(buf.get(0, 2).unwrap().content.as_char(), Some('-'));
        assert_eq!(buf.get(3, 2).unwrap().content.as_char(), Some('+'));
        assert_eq!(buf.get(3, 1).unwrap().content.as_char(), Some('|'));
    }

    #[test]
    fn degradation_no_styling_renders_title_without_styles() {
        use ftui_render::budget::DegradationLevel;

        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(PackedRgba::rgb(200, 0, 0)))
            .title("Hi");
        let area = Rect::new(0, 0, 6, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(6, 3, &mut pool);
        frame.set_degradation(DegradationLevel::NoStyling);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        let default_fg = Cell::default().fg;
        assert_eq!(buf.get(1, 0).unwrap().content.as_char(), Some('H'));
        assert_eq!(buf.get(1, 0).unwrap().fg, default_fg);
    }

    #[test]
    fn degradation_no_styling_keeps_border_when_title_does_not_fit() {
        use ftui_render::budget::DegradationLevel;

        let titled = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .title("界");
        let plain = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square);
        let area = Rect::new(0, 0, 3, 3);

        let mut titled_pool = GraphemePool::new();
        let mut titled_frame = Frame::new(3, 3, &mut titled_pool);
        titled_frame.set_degradation(DegradationLevel::NoStyling);
        titled.render(area, &mut titled_frame);

        let mut plain_pool = GraphemePool::new();
        let mut plain_frame = Frame::new(3, 3, &mut plain_pool);
        plain_frame.set_degradation(DegradationLevel::NoStyling);
        plain.render(area, &mut plain_frame);

        assert_eq!(titled_frame.buffer.get(1, 0), plain_frame.buffer.get(1, 0));
    }

    #[test]
    fn degradation_no_styling_drops_border_style_everywhere() {
        use ftui_render::budget::DegradationLevel;

        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(PackedRgba::rgb(200, 0, 0)).bold());
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        frame.set_degradation(DegradationLevel::NoStyling);
        block.render(area, &mut frame);

        let border = frame.buffer.get(0, 0).unwrap();
        let default_cell = Cell::from_char(border.content.as_char().unwrap());
        assert_eq!(border.fg, default_cell.fg);
        assert_eq!(border.bg, default_cell.bg);
        assert_eq!(border.attrs, default_cell.attrs);
    }

    #[test]
    fn degradation_essential_only_clears_stale_borders_and_title() {
        use ftui_render::budget::DegradationLevel;

        let block = Block::bordered()
            .border_type(BorderType::Square)
            .title("Hi");
        let area = Rect::new(0, 0, 6, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(6, 3, &mut pool);
        block.render(area, &mut frame);

        frame.set_degradation(DegradationLevel::EssentialOnly);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        for y in 0..area.height {
            for x in 0..area.width {
                assert!(
                    buf.get(x, y).unwrap().is_empty(),
                    "expected cleared cell at ({x}, {y}), got {:?}",
                    buf.get(x, y).unwrap()
                );
            }
        }
    }

    #[test]
    fn degradation_skeleton_clears_area() {
        use ftui_render::budget::DegradationLevel;

        let block = Block::bordered();
        let area = Rect::new(0, 0, 3, 2);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(3, 2, &mut pool);
        frame.buffer.fill(area, Cell::from_char('X'));
        frame.set_degradation(DegradationLevel::Skeleton);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert!(buf.get(0, 0).unwrap().is_empty());
    }

    #[test]
    fn block_equality() {
        let a = Block::new().borders(Borders::ALL).title("Test");
        let b = Block::new().borders(Borders::ALL).title("Test");
        assert_eq!(a, b);
    }

    #[test]
    fn render_1x1_no_panic() {
        let block = Block::bordered();
        let area = Rect::new(0, 0, 1, 1);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        block.render(area, &mut frame);
    }

    #[test]
    fn render_2x2_with_borders() {
        let block = Block::bordered().border_type(BorderType::Square);
        let area = Rect::new(0, 0, 2, 2);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(2, 2, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('┌'));
        assert_eq!(buf.get(1, 0).unwrap().content.as_char(), Some('┐'));
        assert_eq!(buf.get(0, 1).unwrap().content.as_char(), Some('└'));
        assert_eq!(buf.get(1, 1).unwrap().content.as_char(), Some('┘'));
    }

    #[test]
    fn title_too_narrow() {
        // Width 3 with all borders = 1 char available for title
        let block = Block::bordered().title("LongTitle");
        let area = Rect::new(0, 0, 4, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(4, 3, &mut pool);
        block.render(area, &mut frame);
        // Should not panic, title gets truncated
    }

    #[test]
    fn alignment_default_is_left() {
        assert_eq!(Alignment::default(), Alignment::Left);
    }

    // --- MeasurableWidget tests ---

    use crate::MeasurableWidget;
    use ftui_core::geometry::Size;

    #[test]
    fn chrome_size_no_borders() {
        let block = Block::new();
        assert_eq!(block.chrome_size(), (0, 0));
    }

    #[test]
    fn chrome_size_all_borders() {
        let block = Block::bordered();
        // Block::bordered() includes 1 cell padding on each side.
        // Chrome = borders (2) + padding (2) = 4 on each axis.
        assert_eq!(block.chrome_size(), (4, 4));
    }

    #[test]
    fn chrome_size_partial_borders() {
        let block = Block::new().borders(Borders::TOP | Borders::LEFT);
        assert_eq!(block.chrome_size(), (1, 1));
    }

    #[test]
    fn chrome_size_horizontal_only() {
        let block = Block::new().borders(Borders::LEFT | Borders::RIGHT);
        assert_eq!(block.chrome_size(), (2, 0));
    }

    #[test]
    fn chrome_size_vertical_only() {
        let block = Block::new().borders(Borders::TOP | Borders::BOTTOM);
        assert_eq!(block.chrome_size(), (0, 2));
    }

    #[test]
    fn measure_no_borders() {
        let block = Block::new();
        let constraints = block.measure(Size::MAX);
        assert_eq!(constraints.min, Size::ZERO);
        assert_eq!(constraints.preferred, Size::ZERO);
    }

    #[test]
    fn measure_all_borders() {
        let block = Block::bordered();
        let constraints = block.measure(Size::MAX);
        assert_eq!(constraints.min, Size::new(4, 4));
        assert_eq!(constraints.preferred, Size::new(4, 4));
        assert_eq!(constraints.max, None); // Unbounded
    }

    #[test]
    fn measure_partial_borders() {
        let block = Block::new().borders(Borders::TOP | Borders::RIGHT);
        let constraints = block.measure(Size::MAX);
        assert_eq!(constraints.min, Size::new(1, 1));
        assert_eq!(constraints.preferred, Size::new(1, 1));
    }

    #[test]
    fn has_intrinsic_size_with_borders() {
        let block = Block::bordered();
        assert!(block.has_intrinsic_size());
    }

    #[test]
    fn has_no_intrinsic_size_without_borders() {
        let block = Block::new();
        assert!(!block.has_intrinsic_size());
    }

    #[test]
    fn measure_is_pure() {
        let block = Block::bordered();
        let a = block.measure(Size::new(100, 50));
        let b = block.measure(Size::new(100, 50));
        assert_eq!(a, b);
    }

    // --- Per-side border style tests ---

    #[test]
    fn border_style_left_applied() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_left(Style::new().fg(PackedRgba::rgb(255, 0, 0)));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().fg, PackedRgba::rgb(255, 0, 0));
        assert_eq!(buf.get(0, 1).unwrap().fg, PackedRgba::rgb(255, 0, 0));
        assert_eq!(buf.get(0, 2).unwrap().fg, PackedRgba::rgb(255, 0, 0));
        assert_ne!(buf.get(4, 0).unwrap().fg, PackedRgba::rgb(255, 0, 0));
        assert_ne!(buf.get(4, 1).unwrap().fg, PackedRgba::rgb(255, 0, 0));
    }

    #[test]
    fn border_style_right_applied() {
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_right(Style::new().fg(red));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(4, 0).unwrap().fg, red);
        assert_eq!(buf.get(4, 1).unwrap().fg, red);
        assert_eq!(buf.get(4, 2).unwrap().fg, red);
        assert_ne!(buf.get(0, 0).unwrap().fg, red);
    }

    #[test]
    fn border_style_top_applied() {
        let blue = PackedRgba::rgb(0, 0, 255);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_top(Style::new().fg(blue));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(1, 0).unwrap().fg, blue);
        assert_eq!(buf.get(2, 0).unwrap().fg, blue);
        assert_eq!(buf.get(3, 0).unwrap().fg, blue);
        assert_ne!(buf.get(1, 2).unwrap().fg, blue);
    }

    #[test]
    fn border_style_bottom_applied() {
        let green = PackedRgba::rgb(0, 255, 0);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_bottom(Style::new().fg(green));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(1, 2).unwrap().fg, green);
        assert_eq!(buf.get(2, 2).unwrap().fg, green);
        assert_eq!(buf.get(3, 2).unwrap().fg, green);
        assert_ne!(buf.get(1, 0).unwrap().fg, green);
    }

    #[test]
    fn border_style_left_corner_uses_left_style() {
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_left(Style::new().fg(red));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().fg, red);
        assert_eq!(buf.get(0, 2).unwrap().fg, red);
    }

    #[test]
    fn border_style_right_corner_uses_right_style() {
        let blue = PackedRgba::rgb(0, 0, 255);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_right(Style::new().fg(blue));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(4, 0).unwrap().fg, blue);
        assert_eq!(buf.get(4, 2).unwrap().fg, blue);
    }

    #[test]
    fn border_style_all_sides_independent() {
        let red = PackedRgba::rgb(255, 0, 0);
        let green = PackedRgba::rgb(0, 255, 0);
        let blue = PackedRgba::rgb(0, 0, 255);
        let white = PackedRgba::rgb(255, 255, 255);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_left(Style::new().fg(red))
            .border_style_right(Style::new().fg(green))
            .border_style_top(Style::new().fg(blue))
            .border_style_bottom(Style::new().fg(white));
        let area = Rect::new(0, 0, 6, 4);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(6, 4, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().fg, red);
        assert_eq!(buf.get(0, 1).unwrap().fg, red);
        assert_eq!(buf.get(0, 3).unwrap().fg, red);
        assert_eq!(buf.get(5, 0).unwrap().fg, green);
        assert_eq!(buf.get(5, 1).unwrap().fg, green);
        assert_eq!(buf.get(5, 3).unwrap().fg, green);
        assert_eq!(buf.get(1, 0).unwrap().fg, blue);
        assert_eq!(buf.get(2, 0).unwrap().fg, blue);
        assert_eq!(buf.get(4, 0).unwrap().fg, blue);
        assert_eq!(buf.get(1, 3).unwrap().fg, white);
        assert_eq!(buf.get(2, 3).unwrap().fg, white);
        assert_eq!(buf.get(4, 3).unwrap().fg, white);
    }

    #[test]
    fn border_style_left_falls_back_to_border_style() {
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style(Style::new().fg(red));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        for x in 0..5 {
            assert_eq!(buf.get(x, 0).unwrap().fg, red);
            assert_eq!(buf.get(x, 2).unwrap().fg, red);
        }
        assert_eq!(buf.get(0, 1).unwrap().fg, red);
        assert_eq!(buf.get(4, 1).unwrap().fg, red);
    }

    #[test]
    fn border_style_override_merges_with_global() {
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style(Style::new().fg(red).bold())
            .border_style_left(Style::new().bg(PackedRgba::rgb(0, 0, 255)));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        let cell = buf.get(0, 0).unwrap();
        assert_eq!(cell.fg, red);
        assert_eq!(cell.bg, PackedRgba::rgb(0, 0, 255));
        let cell_r = buf.get(4, 0).unwrap();
        assert_eq!(cell_r.fg, red);
        assert_ne!(cell_r.bg, PackedRgba::rgb(0, 0, 255));
    }

    #[test]
    fn border_style_per_side_with_no_border_on_that_side() {
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::TOP)
            .border_type(BorderType::Square)
            .border_style_left(Style::new().fg(red));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        let cell = buf.get(0, 0).unwrap();
        assert_ne!(cell.fg, red);
    }

    #[test]
    fn border_style_per_side_partial_borders() {
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_type(BorderType::Square)
            .border_style_left(Style::new().fg(red));
        let area = Rect::new(0, 0, 4, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(4, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().fg, red);
        assert_eq!(buf.get(0, 1).unwrap().fg, red);
        assert_eq!(buf.get(0, 2).unwrap().fg, red);
        assert_ne!(buf.get(3, 0).unwrap().fg, red);
        assert_ne!(buf.get(3, 1).unwrap().fg, red);
        assert_ne!(buf.get(3, 2).unwrap().fg, red);
    }

    #[test]
    fn border_style_title_uses_top_style() {
        let blue = PackedRgba::rgb(0, 0, 255);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_top(Style::new().fg(blue))
            .title("Hi");
        let area = Rect::new(0, 0, 10, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(1, 0).unwrap().fg, blue);
        assert_eq!(buf.get(2, 0).unwrap().fg, blue);
    }

    #[test]
    fn border_style_title_falls_back_to_border_style() {
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style(Style::new().fg(red))
            .title("Hi");
        let area = Rect::new(0, 0, 10, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(1, 0).unwrap().fg, red);
        assert_eq!(buf.get(2, 0).unwrap().fg, red);
    }

    #[test]
    fn border_style_ascii_degradation_per_side() {
        use ftui_render::budget::DegradationLevel;
        let red = PackedRgba::rgb(255, 0, 0);
        let blue = PackedRgba::rgb(0, 0, 255);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style_left(Style::new().fg(red))
            .border_style_right(Style::new().fg(blue));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        frame.set_degradation(DegradationLevel::SimpleBorders);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().fg, red);
        assert_eq!(buf.get(0, 1).unwrap().fg, red);
        assert_eq!(buf.get(0, 2).unwrap().fg, red);
        assert_eq!(buf.get(4, 0).unwrap().fg, blue);
        assert_eq!(buf.get(4, 1).unwrap().fg, blue);
        assert_eq!(buf.get(4, 2).unwrap().fg, blue);
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('+'));
        assert_eq!(buf.get(4, 0).unwrap().content.as_char(), Some('+'));
    }

    #[test]
    fn border_style_per_side_default_not_set() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square);
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().content.as_char(), Some('┌'));
        assert_eq!(buf.get(4, 0).unwrap().content.as_char(), Some('┐'));
    }

    #[test]
    fn border_style_all_per_side_different_bg() {
        let red = PackedRgba::rgb(255, 0, 0);
        let green = PackedRgba::rgb(0, 255, 0);
        let blue = PackedRgba::rgb(0, 0, 255);
        let white = PackedRgba::rgb(255, 255, 255);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_left(Style::new().bg(red))
            .border_style_right(Style::new().bg(green))
            .border_style_top(Style::new().bg(blue))
            .border_style_bottom(Style::new().bg(white));
        let area = Rect::new(0, 0, 6, 4);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(6, 4, &mut pool);
        block.render(area, &mut frame);

        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().bg, red);
        assert_eq!(buf.get(0, 1).unwrap().bg, red);
        assert_eq!(buf.get(0, 3).unwrap().bg, red);
        assert_eq!(buf.get(5, 0).unwrap().bg, green);
        assert_eq!(buf.get(5, 1).unwrap().bg, green);
        assert_eq!(buf.get(5, 3).unwrap().bg, green);
        assert_eq!(buf.get(1, 0).unwrap().bg, blue);
        assert_eq!(buf.get(2, 0).unwrap().bg, blue);
        assert_eq!(buf.get(4, 0).unwrap().bg, blue);
        assert_eq!(buf.get(1, 3).unwrap().bg, white);
        assert_eq!(buf.get(2, 3).unwrap().bg, white);
        assert_eq!(buf.get(4, 3).unwrap().bg, white);
    }

    #[test]
    fn border_style_per_side_equality() {
        let a = Block::new()
            .borders(Borders::ALL)
            .border_style_left(Style::new().fg(PackedRgba::rgb(255, 0, 0)));
        let b = Block::new()
            .borders(Borders::ALL)
            .border_style_left(Style::new().fg(PackedRgba::rgb(255, 0, 0)));
        assert_eq!(a, b);

        let c = Block::new()
            .borders(Borders::ALL)
            .border_style_left(Style::new().fg(PackedRgba::rgb(0, 255, 0)));
        assert_ne!(a, c);
    }

    #[test]
    fn border_style_per_side_does_not_affect_inner() {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style_left(Style::new().fg(PackedRgba::rgb(255, 0, 0)))
            .border_style_right(Style::new().fg(PackedRgba::rgb(0, 255, 0)));
        let area = Rect::new(0, 0, 10, 10);
        let inner = block.inner(area);
        assert_eq!(inner, Rect::new(1, 1, 8, 8));
    }

    #[test]
    fn border_style_per_side_no_degradation() {
        use ftui_render::budget::DegradationLevel;
        let red = PackedRgba::rgb(255, 0, 0);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Square)
            .border_style_left(Style::new().fg(red))
            .border_style_right(Style::new().fg(red));
        let area = Rect::new(0, 0, 5, 3);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 3, &mut pool);
        frame.set_degradation(DegradationLevel::NoStyling);
        block.render(area, &mut frame);

        let default_cell = Cell::default();
        let buf = &frame.buffer;
        assert_eq!(buf.get(0, 0).unwrap().fg, default_cell.fg);
        assert_eq!(buf.get(4, 0).unwrap().fg, default_cell.fg);
    }
}
