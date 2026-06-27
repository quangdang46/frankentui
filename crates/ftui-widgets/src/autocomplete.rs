#![forbid(unsafe_code)]

//! Autocomplete suggestion popup widget.
//!
//! Provides a [`Suggestion`] item type and an [`Autocomplete`] widget that
//! renders a filtered list of suggestions as a bordered popover anchored to
//! an input area.
//!
//! # Navigation
//!
//! - **Up/Down**: Navigate through suggestions
//! - **Enter / Tab**: Accept the currently highlighted suggestion
//! - **Escape**: Close the popup
//!
//! # Filtering
//!
//! Call [`Autocomplete::match_filter`] with a query string to produce a
//! filtered subset. The filter is case-insensitive substring matching
//! against the suggestion label.
//!
//! # Zero height
//!
//! When the filtered list is empty, the widget renders at zero height
//! (no popover is drawn).
//!
//! # Example
//!
//! ```ignore
//! use ftui_widgets::autocomplete::{Autocomplete, Suggestion, AutocompleteAction};
//!
//! let mut ac = Autocomplete::new(suggestions, anchor_rect)
//!     .max_height(8)
//!     .popup_width(30);
//!
//! // Filter suggestions based on query
//! ac.match_filter("ope");
//!
//! // Handle keyboard events
//! match ac.handle_event(&event) {
//!     Some(AutocompleteAction::Accept(idx)) => { /* use selected suggestion */ }
//!     Some(AutocompleteAction::Dismiss) => { /* close popup */ }
//!     None => { /* pass event through */ }
//! }
//!
//! // Render
//! ac.render(area, &mut frame);
//! ```

use ftui_core::event::{Event, KeyCode, KeyEventKind};
use ftui_core::geometry::Rect;
use ftui_render::cell::{Cell, PackedRgba};
use ftui_render::frame::Frame;
use ftui_style::Style;
use ftui_text::display_width;

use crate::popover::{Placement, Popover};
use crate::Widget;

/// A single suggestion item in the autocomplete popup.

#[derive(Clone, Debug)]
pub struct Suggestion {
    /// Primary display text.
    pub label: String,
    /// Optional secondary description shown beside the label.
    pub description: Option<String>,
    /// Optional keyboard shortcut hint (e.g. "Ctrl+P").
    pub shortcut: Option<String>,
    /// Optional icon / prefix character.
    pub icon: Option<char>,
}

impl Suggestion {
    /// Create a new suggestion with just a label.

    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: None,
            shortcut: None,
            icon: None,
        }
    }

    /// Set the description text.

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the keyboard shortcut hint.

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Set the icon character.

    pub fn with_icon(mut self, icon: char) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl From<&str> for Suggestion {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Actions that can be produced by the autocomplete widget.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutocompleteAction {
    /// The suggestion at the given index was accepted (Enter/Tab).
    Accept(usize),
    /// The popup was dismissed (Escape).
    Dismiss,
}

/// Autocomplete suggestion popup widget.
///
/// Renders a bordered list of suggestions as a popover anchored to the
/// provided anchor rectangle. Provides keyboard navigation and filtering.

pub struct Autocomplete {
    /// All registered suggestions (unfiltered).
    suggestions: Vec<Suggestion>,
    /// Indices into `suggestions` that pass the current filter.
    filtered: Vec<usize>,
    /// Currently highlighted index within `filtered`.
    selected: usize,
    /// The anchor rectangle (typically the input field area).
    anchor: Rect,
    /// Desired popup width (defaults to anchor width if not set).
    popup_width: Option<u16>,
    /// Maximum number of visible items (affects popup height).
    max_height: u16,
    /// Vertical scroll offset when filtered items exceed max_height.
    scroll_offset: u16,
    /// Whether the popup is currently visible.
    visible: bool,
    /// Highlighted item style.
    highlight_style: Style,
    /// Normal item style.
    normal_style: Style,
    /// Icon style.
    icon_style: Style,
    /// Shortcut style.
    shortcut_style: Style,
    /// Border style applied to the popover.
    bordered: bool,
}

impl Autocomplete {
    /// Create a new autocomplete widget anchored to the given rectangle.
    ///
    /// The anchor should be the area of the text input or widget that
    /// triggers the autocomplete suggestions.

    pub fn new(suggestions: Vec<Suggestion>, anchor: Rect) -> Self {
        let count = suggestions.len();
        Self {
            suggestions,
            filtered: (0..count).collect(),
            selected: 0,
            anchor,
            popup_width: None,
            max_height: 8,
            scroll_offset: 0,
            visible: true,
            highlight_style: Style::new()
                .bg(PackedRgba::rgba(80, 80, 120, 180))
                .fg(PackedRgba::rgb(255, 255, 255)),
            normal_style: Style::default(),
            icon_style: Style::new().fg(PackedRgba::rgb(150, 150, 200)),
            shortcut_style: Style::new().fg(PackedRgba::rgb(120, 120, 120)),
            bordered: true,
        }
    }

    /// Set the popup width (if not set, uses anchor width).

    pub fn popup_width(mut self, w: u16) -> Self {
        self.popup_width = Some(w);
        self
    }

    /// Set the maximum number of visible items.

    pub fn max_height(mut self, h: u16) -> Self {
        self.max_height = h;
        self
    }

    /// Set the highlight (selected item) style.

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Set the normal (unselected) item style.

    pub fn normal_style(mut self, style: Style) -> Self {
        self.normal_style = style;
        self
    }

    /// Set the icon style.

    pub fn icon_style(mut self, style: Style) -> Self {
        self.icon_style = style;
        self
    }

    /// Set the shortcut hint style.

    pub fn shortcut_style(mut self, style: Style) -> Self {
        self.shortcut_style = style;
        self
    }

    /// Enable or disable the border.

    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    /// Show or hide the popup.

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.selected = 0;
            self.scroll_offset = 0;
        }
    }

    /// Returns whether the popup is currently visible.

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Returns the number of filtered (visible) suggestions.

    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }

    /// Returns the currently highlighted suggestion, if any.

    pub fn selected_suggestion(&self) -> Option<&Suggestion> {
        self.filtered
            .get(self.selected)
            .map(|&idx| &self.suggestions[idx])
    }

    /// Returns the index into the original (unfiltered) suggestions array
    /// for the currently highlighted item.

    pub fn selected_original_index(&self) -> Option<usize> {
        self.filtered.get(self.selected).copied()
    }

    /// Filter suggestions by case-insensitive substring match on the label.
    ///
    /// An empty query returns all suggestions. After filtering, the selection
    /// is clamped to the new range and the scroll offset is reset.

    pub fn match_filter(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        self.filtered = self
            .suggestions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if query_lower.is_empty() {
                    return true;
                }
                s.label.to_lowercase().contains(&query_lower)
            })
            .map(|(i, _)| i)
            .collect();
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        self.scroll_offset = 0;
    }

    /// Handle an input event and return an action if one occurred.
    ///
    /// Returns `None` when the event should be passed through to the
    /// underlying widget.

    pub fn handle_event(&mut self, event: &Event) -> Option<AutocompleteAction> {
        if !self.visible || self.filtered.is_empty() {
            return None;
        }

        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press {
            return None;
        }

        match key.code {
            KeyCode::Down => {
                if self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                    self.ensure_selected_visible();
                }
                None
            }
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.ensure_selected_visible();
                }
                None
            }
            KeyCode::Enter | KeyCode::Tab => {
                let idx = self.selected_original_index()?;
                self.visible = false;
                Some(AutocompleteAction::Accept(idx))
            }
            KeyCode::Escape => {
                self.visible = false;
                Some(AutocompleteAction::Dismiss)
            }
            _ => None,
        }
    }

    /// Adjust scroll offset so the selected item is visible.

    fn ensure_selected_visible(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let selected_row = self.selected as u16;
        if selected_row < self.scroll_offset {
            self.scroll_offset = selected_row;
        } else if selected_row >= self.scroll_offset + self.max_height {
            self.scroll_offset = selected_row.saturating_sub(self.max_height).saturating_add(1);
        }
    }

    /// Get all registered suggestions (unfiltered).

    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }

    /// Get the mutable filtered index list.

    pub fn filtered_indices(&self) -> &[usize] {
        &self.filtered
    }

    /// Replace the full suggestion list and re-filter.

    pub fn set_suggestions(&mut self, suggestions: Vec<Suggestion>, query: &str) {
        self.suggestions = suggestions;
        self.match_filter(query);
    }
}

impl Widget for Autocomplete {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if !self.visible || self.filtered.is_empty() {
            return;
        }

        let content_width = self
            .popup_width
            .unwrap_or(self.anchor.width)
            .max(10);

        let item_count = self.filtered.len().min(self.max_height as usize);
        if item_count == 0 {
            return;
        }

        let popover = Popover::new(self.anchor, Placement::Below)
            .width(content_width)
            .max_height(item_count as u16)
            .with_border(self.bordered)
            .gap(0);

        popover.render_with(area, frame, |content_area, frame| {
            if content_area.is_empty() {
                return;
            }

            let max_label_width = content_area.width.saturating_sub(2).max(1);

            for i in 0..item_count {
                let row_y = content_area.y + i as u16;
                if row_y >= content_area.y + content_area.height {
                    break;
                }

                let actual_idx = self.scroll_offset as usize + i;
                if actual_idx >= self.filtered.len() {
                    break;
                }

                let sug_idx = self.filtered[actual_idx];
                let suggestion = &self.suggestions[sug_idx];
                let is_selected = actual_idx == self.selected;

                // Clear the row first
                for col in 0..content_area.width {
                    frame.buffer.set_fast(
                        content_area.x + col,
                        row_y,
                        Cell::from_char(' '),
                    );
                }

                let mut col = content_area.x;

                // Icon
                if let Some(icon) = suggestion.icon {
                    if col < content_area.x + content_area.width {
                        let style = if is_selected {
                            self.highlight_style
                        } else {
                            self.icon_style
                        };
                        let icon_cell = Cell::from_char(icon);
                        let fg = style.fg.unwrap_or(icon_cell.fg);
                        let bg = if is_selected {
                            self.highlight_style.bg.unwrap_or(icon_cell.bg)
                        } else {
                            icon_cell.bg
                        };
                        frame.buffer.set_fast(
                            col,
                            row_y,
                            Cell::from_char(icon).with_fg(fg).with_bg(bg),
                        );
                        col = col.saturating_add(2);
                    }
                }

                // Label (truncated to fit)
                let available = max_label_width.saturating_sub(col.saturating_sub(content_area.x));
                if available > 0 && col < content_area.x + content_area.width {
                    let display_w = display_width(&suggestion.label) as u16;
                    let label: String = if display_w > available {
                        truncate_label(&suggestion.label, available as usize)
                    } else {
                        suggestion.label.clone()
                    };

                    let mut cell = Cell::from_char(' ');
                    let style = if is_selected {
                        self.highlight_style
                    } else {
                        self.normal_style
                    };
                    if let Some(fg) = style.fg {
                        cell.fg = fg;
                    }
                    if let Some(bg) = style.bg {
                        cell.bg = bg;
                    }
                    for ch in label.chars() {
                        if col >= content_area.x + content_area.width {
                            break;
                        }
                        cell.content = ftui_render::cell::CellContent::from_char(ch);
                        frame.buffer.set_fast(col, row_y, cell);
                        col = col.saturating_add(1);
                    }
                }

                // Shortcut hint (right-aligned)
                if let Some(ref shortcut) = suggestion.shortcut {
                    let sc_w = display_width(shortcut) as u16;
                    let shortcut_x = content_area
                        .x
                        .saturating_add(content_area.width)
                        .saturating_sub(sc_w)
                        .saturating_sub(1);
                    if shortcut_x > col + 2 && shortcut_x >= content_area.x {
                        let style = if is_selected {
                            self.highlight_style
                        } else {
                            self.shortcut_style
                        };
                        let mut sc_cell = Cell::from_char(' ');
                        if let Some(fg) = style.fg {
                            sc_cell.fg = fg;
                        }
                        if let Some(bg) = style.bg {
                            sc_cell.bg = bg;
                        }
                        for (j, ch) in shortcut.chars().enumerate() {
                            let sx = shortcut_x + j as u16;
                            if sx >= content_area.x + content_area.width {
                                break;
                            }
                            sc_cell.content = ftui_render::cell::CellContent::from_char(ch);
                            frame.buffer.set_fast(sx, row_y, sc_cell);
                        }
                    }
                }
            }
        });
    }

    fn is_essential(&self) -> bool {
        false
    }
}

/// Truncate a label string to fit within the given display width, adding "..." if truncated.

fn truncate_label(s: &str, max_width: usize) -> String {
    if max_width < 4 {
        return s.chars().take(max_width.max(1)).collect();
    }
    let display_w = display_width(s);
    if display_w <= max_width {
        return s.to_string();
    }

    // Reserve space for "..."
    let available = max_width.saturating_sub(3);
    if available == 0 {
        return s.chars().take(max_width).collect();
    }

    let mut result = String::with_capacity(s.len());
    let mut w = 0;
    for ch in s.chars() {
        let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + ch_w > available {
            break;
        }
        result.push(ch);
        w += ch_w;
    }
    result.push_str("...");
    result
}

mod tests {
    use super::*;
    use ftui_core::event::{KeyCode, KeyEvent, KeyEventKind, Modifiers};
    use ftui_render::frame::Frame;
    use ftui_render::grapheme_pool::GraphemePool;

    // --- Suggestion construction ---

    fn suggestion_new_label_only() {
        let s = Suggestion::new("Open File");
        assert_eq!(s.label, "Open File");
        assert!(s.description.is_none());
        assert!(s.shortcut.is_none());
        assert!(s.icon.is_none());
    }

    fn suggestion_with_description() {
        let s = Suggestion::new("Save").with_description("Save current file");
        assert_eq!(s.label, "Save");
        assert_eq!(s.description, Some("Save current file".into()));
    }

    fn suggestion_with_shortcut() {
        let s = Suggestion::new("Copy").with_shortcut("Ctrl+C");
        assert_eq!(s.shortcut, Some("Ctrl+C".into()));
    }

    fn suggestion_with_icon() {
        let s = Suggestion::new("Delete").with_icon('D');
        assert_eq!(s.icon, Some('D'));
    }

    fn suggestion_from_str() {
        let s: Suggestion = "Simple".into();
        assert_eq!(s.label, "Simple");
    }

    fn suggestion_full_construction() {
        let s = Suggestion::new("Find")
            .with_description("Search in file")
            .with_shortcut("Ctrl+F")
            .with_icon('F');
        assert_eq!(s.label, "Find");
        assert_eq!(s.description.as_deref(), Some("Search in file"));
        assert_eq!(s.shortcut.as_deref(), Some("Ctrl+F"));
        assert_eq!(s.icon, Some('F'));
    }

    fn suggestion_debug_format() {
        let s = Suggestion::new("test");
        let dbg = format!("{s:?}");
        assert!(dbg.contains("test"));
    }

    fn suggestion_clone() {
        let s = Suggestion::new("A").with_shortcut("Ctrl+A");
        let c = s.clone();
        assert_eq!(c.label, "A");
        assert_eq!(c.shortcut, Some("Ctrl+A".into()));
    }

    // --- Autocomplete construction ---

    fn autocomplete_creation() {
        let suggestions = vec![
            Suggestion::new("Open"),
            Suggestion::new("Save"),
            Suggestion::new("Close"),
        ];
        let anchor = Rect::new(10, 5, 20, 1);
        let ac = Autocomplete::new(suggestions, anchor);

        assert_eq!(ac.suggestions().len(), 3);
        assert_eq!(ac.filtered_count(), 3);
        assert!(ac.is_visible());
        assert_eq!(ac.selected_original_index(), Some(0));
    }

    fn autocomplete_empty_suggestions() {
        let ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1));
        assert_eq!(ac.filtered_count(), 0);
        assert!(ac.is_visible());
    }

    fn autocomplete_default_max_height() {
        let ac = Autocomplete::new(vec![Suggestion::new("X")], Rect::new(0, 0, 10, 1));
        assert_eq!(ac.max_height, 8);
    }

    fn autocomplete_popup_width_builder() {
        let ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1)).popup_width(30);
        assert_eq!(ac.popup_width, Some(30));
    }

    fn autocomplete_max_height_builder() {
        let ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1)).max_height(5);
        assert_eq!(ac.max_height, 5);
    }

    fn autocomplete_bordered_builder() {
        let ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1)).bordered(false);
        assert!(!ac.bordered);
    }

    fn autocomplete_style_builders() {
        let ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1))
            .highlight_style(Style::new().bg(PackedRgba::rgb(1, 2, 3)))
            .normal_style(Style::new().fg(PackedRgba::rgb(4, 5, 6)))
            .icon_style(Style::new().fg(PackedRgba::rgb(7, 8, 9)))
            .shortcut_style(Style::new().fg(PackedRgba::rgb(10, 11, 12)));
        assert!(ac.highlight_style.bg.is_some());
        assert!(ac.normal_style.fg.is_some());
        assert!(ac.icon_style.fg.is_some());
        assert!(ac.shortcut_style.fg.is_some());
    }

    fn autocomplete_set_visible_false() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A"), Suggestion::new("B")],
            Rect::new(0, 0, 10, 1),
        );
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down))); // select index 1
        ac.set_visible(false);
        assert!(!ac.is_visible());
        assert_eq!(ac.selected, 0);
    }

    fn autocomplete_set_visible_true() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A")],
            Rect::new(0, 0, 10, 1),
        );
        ac.set_visible(true);
        assert!(ac.is_visible());
    }

    fn autocomplete_selected_suggestion() {
        let suggestions = vec![
            Suggestion::new("First"),
            Suggestion::new("Second"),
        ];
        let mut ac = Autocomplete::new(suggestions, Rect::new(0, 0, 10, 1));
        assert_eq!(ac.selected_suggestion().unwrap().label, "First");

        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected_suggestion().unwrap().label, "Second");
    }

    fn autocomplete_selected_suggestion_empty() {
        let ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1));
        assert!(ac.selected_suggestion().is_none());
    }

    fn autocomplete_selected_original_index() {
        let suggestions = vec![
            Suggestion::new("Apple"),
            Suggestion::new("Banana"),
            Suggestion::new("Apricot"),
        ];
        let mut ac = Autocomplete::new(suggestions, Rect::new(0, 0, 10, 1));

        // After filtering for "Ap", indices 0 (Apple) and 2 (Apricot) match
        ac.match_filter("Ap");
        assert_eq!(ac.filtered_count(), 2);
        assert_eq!(ac.selected_original_index(), Some(0));

        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected_original_index(), Some(2));
    }

    // --- match_filter ---

    fn match_filter_empty_query_returns_all() {
        let mut ac = Autocomplete::new(
            vec![
                Suggestion::new("Open"),
                Suggestion::new("Save"),
                Suggestion::new("Close"),
            ],
            Rect::new(0, 0, 10, 1),
        );
        ac.match_filter("");
        assert_eq!(ac.filtered_count(), 3);
    }

    fn match_filter_case_insensitive() {
        let mut ac = Autocomplete::new(
            vec![
                Suggestion::new("Open File"),
                Suggestion::new("Save As"),
                Suggestion::new("Open Folder"),
                Suggestion::new("Close"),
            ],
            Rect::new(0, 0, 10, 1),
        );
        ac.match_filter("open");
        assert_eq!(ac.filtered_count(), 2);
        assert_eq!(ac.suggestions()[ac.filtered[0]].label, "Open File");
        assert_eq!(ac.suggestions()[ac.filtered[1]].label, "Open Folder");
    }

    fn match_filter_partial_substring() {
        let mut ac = Autocomplete::new(
            vec![
                Suggestion::new("Rename"),
                Suggestion::new("Undo"),
                Suggestion::new("Redo"),
                Suggestion::new("Find"),
            ],
            Rect::new(0, 0, 10, 1),
        );
        ac.match_filter("Re");
        assert_eq!(ac.filtered_count(), 2);
        assert_eq!(ac.suggestions()[ac.filtered[0]].label, "Rename");
        assert_eq!(ac.suggestions()[ac.filtered[1]].label, "Redo");
    }

    fn match_filter_no_match() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Open"), Suggestion::new("Save")],
            Rect::new(0, 0, 10, 1),
        );
        ac.match_filter("zzzz");
        assert_eq!(ac.filtered_count(), 0);
    }

    fn match_filter_clamps_selection() {
        let mut ac = Autocomplete::new(
            vec![
                Suggestion::new("A"),
                Suggestion::new("B"),
                Suggestion::new("C"),
            ],
            Rect::new(0, 0, 10, 1),
        );
        // Move to last item
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected, 2);

        // Filter to single item — selection should clamp to 0
        ac.match_filter("A");
        assert_eq!(ac.selected, 0);
        assert_eq!(ac.filtered_count(), 1);
    }

    // --- handle_event ---

    fn event_down_moves_selection() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A"), Suggestion::new("B"), Suggestion::new("C")],
            Rect::new(0, 0, 10, 1),
        );
        assert_eq!(ac.selected, 0);

        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected, 1);

        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected, 2);
    }

    fn event_down_clamps_at_bottom() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A")],
            Rect::new(0, 0, 10, 1),
        );
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected, 0);
    }

    fn event_up_moves_selection() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A"), Suggestion::new("B")],
            Rect::new(0, 0, 10, 1),
        );
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected, 1);

        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Up)));
        assert_eq!(ac.selected, 0);
    }

    fn event_up_clamps_at_top() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A")],
            Rect::new(0, 0, 10, 1),
        );
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Up)));
        assert_eq!(ac.selected, 0);
    }

    fn event_enter_accepts() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Pick Me")],
            Rect::new(0, 0, 10, 1),
        );
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Enter)));
        assert_eq!(action, Some(AutocompleteAction::Accept(0)));
        assert!(!ac.is_visible());
    }

    fn event_tab_accepts() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Pick Me")],
            Rect::new(0, 0, 10, 1),
        );
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Tab)));
        assert_eq!(action, Some(AutocompleteAction::Accept(0)));
        assert!(!ac.is_visible());
    }

    fn event_escape_dismisses() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Item")],
            Rect::new(0, 0, 10, 1),
        );
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Escape)));
        assert_eq!(action, Some(AutocompleteAction::Dismiss));
        assert!(!ac.is_visible());
    }

    fn event_returns_none_for_non_nav_keys() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Item")],
            Rect::new(0, 0, 10, 1),
        );
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('a'))));
        assert_eq!(action, None);
        assert!(ac.is_visible());
    }

    fn event_non_key_returns_none() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Item")],
            Rect::new(0, 0, 10, 1),
        );

        use ftui_core::event::MouseEvent as FtuiMouseEvent;
        use ftui_core::event::MouseButton as FtuiMouseButton;
        use ftui_core::event::MouseEventKind as FtuiMouseEventKind;

        let mouse_event = Event::Mouse(FtuiMouseEvent::new(
            FtuiMouseEventKind::Down(FtuiMouseButton::Left),
            5,
            3,
        ));
        let action = ac.handle_event(&mouse_event);
        assert_eq!(action, None);
    }

    fn event_when_hidden_returns_none() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Item")],
            Rect::new(0, 0, 10, 1),
        );
        ac.set_visible(false);
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Enter)));
        assert_eq!(action, None);
    }

    fn event_when_empty_returns_none() {
        let mut ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1));
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Enter)));
        assert_eq!(action, None);
    }

    fn event_accept_respects_filter_order() {
        let mut ac = Autocomplete::new(
            vec![
                Suggestion::new("Banana"),
                Suggestion::new("Apple"),
                Suggestion::new("Apricot"),
            ],
            Rect::new(0, 0, 10, 1),
        );
        ac.match_filter("Ap");
        // filtered should be [1 (Apple), 2 (Apricot)]
        assert_eq!(ac.filtered, vec![1, 2]);

        // Accept should return the original index of the highlighted item
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Enter)));
        assert_eq!(action, Some(AutocompleteAction::Accept(1))); // Apple at original index 1

        ac.match_filter("Ap");
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        let action = ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Enter)));
        assert_eq!(action, Some(AutocompleteAction::Accept(2))); // Apricot at original index 2
    }

    fn event_key_repeat_does_not_trigger() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A")],
            Rect::new(0, 0, 10, 1),
        );
        let repeat_event = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: Modifiers::NONE,
            kind: KeyEventKind::Repeat,
        });
        let action = ac.handle_event(&repeat_event);
        assert_eq!(action, None);
    }

    fn event_key_release_does_not_trigger() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A")],
            Rect::new(0, 0, 10, 1),
        );
        let release_event = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: Modifiers::NONE,
            kind: KeyEventKind::Release,
        });
        let action = ac.handle_event(&release_event);
        assert_eq!(action, None);
    }

    // --- set_suggestions ---

    fn set_suggestions_replaces_and_filters() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Old")],
            Rect::new(0, 0, 10, 1),
        );
        ac.set_suggestions(
            vec![
                Suggestion::new("New1"),
                Suggestion::new("New2"),
                Suggestion::new("Different"),
            ],
            "New",
        );
        assert_eq!(ac.filtered_count(), 2);
        assert_eq!(ac.suggestions().len(), 3);
    }

    fn set_suggestions_preserves_selection_clamp() {
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("A"), Suggestion::new("B")],
            Rect::new(0, 0, 10, 1),
        );
        ac.handle_event(&Event::Key(KeyEvent::new(KeyCode::Down)));
        assert_eq!(ac.selected, 1);

        // Replace with a single-item list — selection should clamp
        ac.set_suggestions(vec![Suggestion::new("Only")], "");
        assert_eq!(ac.selected, 0);
        assert_eq!(ac.filtered_count(), 1);
    }

    // --- ensure_selected_visible ---

    fn ensure_selected_visible_does_not_scroll_unnecessarily() {
        let mut ac = Autocomplete::new(
            (0..20).map(|i| Suggestion::new(format!("Item {i}"))).collect(),
            Rect::new(0, 0, 10, 1),
        );
        ac.max_height = 5;
        // Selection starts at 0, scroll_offset should be 0
        ac.ensure_selected_visible();
        assert_eq!(ac.scroll_offset, 0);
    }

    fn ensure_selected_visible_scrolls_down() {
        let mut ac = Autocomplete::new(
            (0..20).map(|i| Suggestion::new(format!("Item {i}"))).collect(),
            Rect::new(0, 0, 10, 1),
        );
        ac.max_height = 5;
        ac.selected = 7;
        ac.ensure_selected_visible();
        // 7 >= 0 + 5, so scroll_offset should be 7 - 5 + 1 = 3
        assert_eq!(ac.scroll_offset, 3);
    }

    fn ensure_selected_visible_scrolls_up() {
        let mut ac = Autocomplete::new(
            (0..20).map(|i| Suggestion::new(format!("Item {i}"))).collect(),
            Rect::new(0, 0, 10, 1),
        );
        ac.max_height = 5;
        ac.selected = 8;
        ac.scroll_offset = 6;
        ac.ensure_selected_visible();
        // selected (8) >= offset (6) and < offset + max_height (11), so no change
        assert_eq!(ac.scroll_offset, 6);

        // Now move selected above scroll_offset
        ac.selected = 5;
        ac.ensure_selected_visible();
        assert_eq!(ac.scroll_offset, 5);
    }

    fn ensure_selected_visible_empty_filtered() {
        let mut ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1));
        ac.selected = 0;
        ac.scroll_offset = 5;
        ac.ensure_selected_visible();
        // No change since filtered is empty
        assert_eq!(ac.scroll_offset, 5);
    }

    // --- render / zero height ---

    fn render_does_nothing_when_not_visible() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 24, &mut pool);
        let ac = Autocomplete::new(
            vec![Suggestion::new("Test")],
            Rect::new(10, 5, 20, 1),
        );
        // Default visible = true, so we test rendering with set_visible(false)
        let mut ac = ac;
        ac.set_visible(false);
        ac.render(Rect::new(0, 0, 80, 24), &mut frame);
        // No crash — this is the main assertion
    }

    fn render_does_nothing_when_filtered_empty() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 24, &mut pool);
        let mut ac = Autocomplete::new(
            vec![Suggestion::new("Test")],
            Rect::new(10, 5, 20, 1),
        );
        ac.match_filter("NO_MATCH");
        ac.render(Rect::new(0, 0, 80, 24), &mut frame);
        // No crash — zero-height case
    }

    fn render_with_suggestions_no_crash() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 24, &mut pool);
        let suggestions = vec![
            Suggestion::new("Open File").with_icon('O').with_shortcut("Ctrl+O"),
            Suggestion::new("Save").with_icon('S').with_shortcut("Ctrl+S"),
            Suggestion::new("Close").with_icon('X').with_shortcut("Ctrl+W"),
        ];
        let ac = Autocomplete::new(suggestions, Rect::new(10, 5, 20, 1))
            .max_height(5);
        ac.render(Rect::new(0, 0, 80, 24), &mut frame);
        // No crash — visual assertion
    }

    fn render_at_viewport_edge_no_crash() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 24, &mut pool);
        let suggestions = vec![Suggestion::new("Item")];
        let ac = Autocomplete::new(suggestions, Rect::new(75, 22, 5, 1));
        ac.render(Rect::new(0, 0, 80, 24), &mut frame);
        // No crash — popover flip logic handles edge cases
    }

    fn is_essential_is_false() {
        let ac = Autocomplete::new(vec![], Rect::new(0, 0, 10, 1));
        assert!(!ac.is_essential());
    }

    // --- filtered_indices ---

    fn filtered_indices_reflects_match_filter() {
        let mut ac = Autocomplete::new(
            vec![
                Suggestion::new("Apple"),
                Suggestion::new("Banana"),
                Suggestion::new("Apricot"),
            ],
            Rect::new(0, 0, 10, 1),
        );
        ac.match_filter("Ap");
        assert_eq!(ac.filtered_indices(), &[0, 2]);
    }

    // --- truncate_label ---

    fn truncate_label_short_string() {
        assert_eq!(truncate_label("Hello", 10), "Hello");
    }

    fn truncate_label_requires_truncation() {
        let result = truncate_label("Hello World", 8);
        assert!(result.ends_with("..."));
        assert!(display_width(&result) <= 8);
    }

    fn truncate_label_very_small_max() {
        let result = truncate_label("Hello", 2);
        assert!(result.len() <= 2);
    }

    fn truncate_label_zero_width() {
        let result = truncate_label("Hello", 0);
        assert_eq!(result, "H");
    }

    fn truncate_label_exact_fit() {
        let result = truncate_label("Hello", 5);
        assert_eq!(result, "Hello");
    }

    fn truncate_label_multibyte() {
        let result = truncate_label("Cafe\u{0301}", 5);
        // The combined character "Cafe\u{0301}" has display width 5,
        // but if truncated, it will handle multibyte gracefully
        assert!(display_width(&result) <= 5);
    }
}
