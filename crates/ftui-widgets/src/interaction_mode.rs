#![forbid(unsafe_code)]

//! Custom interaction modes system.
//!
//! Provides distinct interaction modes (Normal, Copy, Command, VisualSelect, Insert)
//! with a manager that tracks transitions and routes key events.
//! Each mode has its own keybindings and can lock events from other widgets.
//!
//! # Example
//!
//! ```ignore
//! use ftui_widgets::interaction_mode::{ModeManager, InteractionMode, KeyAction};
//!
//! let mut modes = ModeManager::new();
//! assert_eq!(modes.current(), InteractionMode::Normal);
//!
//! // Transition to Copy mode
//! modes.transition(InteractionMode::Copy);
//! assert_eq!(modes.current(), InteractionMode::Copy);
//!
//! // Esc returns to Normal
//! modes.handle_escape();
//! assert_eq!(modes.current(), InteractionMode::Normal);
//! ```

use crate::{Widget, draw_text_span};
use ftui_core::event::{KeyCode, KeyEvent, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_render::frame::Frame;
use ftui_render::cell::PackedRgba;
use ftui_style::Style;
use ftui_text::display_width;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// InteractionMode enum
// ---------------------------------------------------------------------------

/// Available interaction modes.
///
/// Each mode defines distinct keybindings and behavior.
/// `Normal` is the default mode; Escape always returns to Normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InteractionMode {
    /// Default mode — standard widget interaction.
    #[default]
    Normal,
    /// Text selection / copy mode (Vim-style visual selection).
    Copy,
    /// Leader-key command mode (like Vim command-line).
    Command,
    /// Visual selection mode for block/line selection.
    VisualSelect,
    /// Insert mode for text input.
    Insert,
}

impl InteractionMode {
    /// Get a short label for the mode indicator pill.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Copy => "COPY",
            Self::Command => "CMD",
            Self::VisualSelect => "VISUAL",
            Self::Insert => "INSERT",
        }
    }

    /// Check if this mode locks key events from regular widgets.
    #[must_use]
    pub fn locks_events(self) -> bool {
        matches!(self, Self::Copy | Self::VisualSelect | Self::Command)
    }

    /// Get a list of common keybinding hints for this mode.
    #[must_use]
    pub fn key_hints(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Normal => &[
                ("i", "Insert mode"),
                ("v", "Visual select"),
                (":", "Command mode"),
                ("Esc", "Clear"),
            ],
            Self::Copy => &[
                ("y", "Copy selection"),
                ("V", "Select line"),
                ("Esc", "Exit copy"),
            ],
            Self::Command => &[
                ("Enter", "Execute"),
                ("Esc", "Cancel"),
            ],
            Self::VisualSelect => &[
                ("y", "Yank"),
                ("d", "Delete"),
                ("Esc", "Exit visual"),
            ],
            Self::Insert => &[
                ("Esc", "Exit insert"),
            ],
        }
    }

    /// Get the accent style for this mode's indicator.
    #[must_use]
    pub fn indicator_style(self) -> Style {
        match self {
            Self::Normal => Style::new().fg(PackedRgba::rgb(100, 180, 100)),
            Self::Copy => Style::new().fg(PackedRgba::rgb(100, 150, 220)),
            Self::Command => Style::new().fg(PackedRgba::rgb(220, 180, 80)),
            Self::VisualSelect => Style::new().fg(PackedRgba::rgb(200, 130, 200)),
            Self::Insert => Style::new().fg(PackedRgba::rgb(180, 200, 100)),
        }
    }
}

// ---------------------------------------------------------------------------
// KeyAction — what a key event does in a mode
// ---------------------------------------------------------------------------

/// Result of routing a key event through a mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// The key was not consumed by the mode system.
    PassThrough,
    /// The key was consumed (no further processing needed).
    Consumed,
    /// Request a mode transition.
    Transition(InteractionMode),
}

// ---------------------------------------------------------------------------
// ModeManager
// ---------------------------------------------------------------------------

/// Manages interaction mode state, transitions, and key routing.
///
/// Maintains a history stack so Escape can pop back to the previous mode.
/// Each mode can define custom keybindings that override widget-level events.
///
/// # Example
///
/// ```ignore
/// let mut mm = ModeManager::new();
/// mm.bind(InteractionMode::Copy, KeyCode::Char('y'), KeyAction::Consumed);
/// ```
#[derive(Debug)]
pub struct ModeManager {
    /// Current active mode.
    current: InteractionMode,
    /// History stack for Escape-to-return navigation.
    history: Vec<InteractionMode>,
    /// Per-mode keybindings: (mode, key) -> action.
    bindings: HashMap<(InteractionMode, KeyCode), KeyAction>,
    /// Whether mode transitions are locked (e.g., during drag).
    locked: bool,
}

impl Default for ModeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModeManager {
    /// Create a new mode manager in Normal mode.
    #[must_use]
    pub fn new() -> Self {
        let mut manager = Self {
            current: InteractionMode::Normal,
            history: Vec::new(),
            bindings: HashMap::new(),
            locked: false,
        };
        // Register default keybindings
        manager.register_defaults();
        manager
    }

    /// Register the default mode-transition keybindings.
    fn register_defaults(&mut self) {
        // Normal mode transitions
        self.bind(InteractionMode::Normal, KeyCode::Char('i'), KeyAction::Transition(InteractionMode::Insert));
        self.bind(InteractionMode::Normal, KeyCode::Char('v'), KeyAction::Transition(InteractionMode::VisualSelect));
        self.bind(InteractionMode::Normal, KeyCode::Char('V'), KeyAction::Transition(InteractionMode::VisualSelect));
        self.bind(InteractionMode::Normal, KeyCode::Char(':'), KeyAction::Transition(InteractionMode::Command));
        // Ctrl+C enters Copy mode
        self.bind(InteractionMode::Normal, KeyCode::Char('c'), KeyAction::Transition(InteractionMode::Copy));

        // Escape always returns to Normal
        for mode in &[
            InteractionMode::Copy,
            InteractionMode::Command,
            InteractionMode::VisualSelect,
            InteractionMode::Insert,
        ] {
            self.bind(*mode, KeyCode::Escape, KeyAction::Transition(InteractionMode::Normal));
        }
    }

    /// Get the current mode.
    #[must_use]
    pub fn current(&self) -> InteractionMode {
        self.current
    }

    /// Check if the manager is currently locked.
    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Lock/unlock mode transitions.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    /// Transition to a new mode, pushing the current mode onto the history stack.
    pub fn transition(&mut self, target: InteractionMode) {
        if self.locked {
            return;
        }
        if self.current == target {
            return;
        }
        self.history.push(self.current);
        self.current = target;
    }

    /// Handle an Escape key — pop back to the previous mode, or stay in Normal.
    ///
    /// Returns `true` if the mode changed.
    pub fn handle_escape(&mut self) -> bool {
        if self.locked {
            return false;
        }
        if self.current == InteractionMode::Normal {
            return false;
        }
        if let Some(previous) = self.history.pop() {
            self.current = previous;
            true
        } else {
            self.current = InteractionMode::Normal;
            true
        }
    }

    /// Bind a key action in a specific mode.
    pub fn bind(&mut self, mode: InteractionMode, key: KeyCode, action: KeyAction) {
        self.bindings.insert((mode, key), action);
    }

    /// Remove a key binding.
    pub fn unbind(&mut self, mode: InteractionMode, key: KeyCode) {
        self.bindings.remove(&(mode, key));
    }

    /// Route a key event through the mode system.
    ///
    /// Returns the action to take.
    #[must_use]
    pub fn route_key(&mut self, event: &KeyEvent) -> KeyAction {
        if event.kind != KeyEventKind::Press {
            return KeyAction::PassThrough;
        }

        // Escape always handled first
        if event.code == KeyCode::Escape {
            if self.handle_escape() {
                return KeyAction::Transition(self.current);
            }
            return KeyAction::PassThrough;
        }

        // Check per-mode bindings
        let key = (self.current, event.code);
        if let Some(action) = self.bindings.get(&key).cloned() {
            match &action {
                KeyAction::Transition(target) => {
                    self.transition(*target);
                }
                _ => {}
            }
            return action;
        }

        KeyAction::PassThrough
    }

    /// Get the mode indicator label with formatting.
    #[must_use]
    pub fn indicator_label(&self) -> String {
        format!("[{}]", self.current.label())
    }

    /// Get the style for the current mode indicator.
    #[must_use]
    pub fn indicator_style(&self) -> Style {
        self.current.indicator_style()
    }

    /// Reset to Normal mode and clear history.
    pub fn reset(&mut self) {
        self.current = InteractionMode::Normal;
        self.history.clear();
    }

    /// Get the depth of the history stack (how many Escape presses to return).
    #[must_use]
    pub fn history_depth(&self) -> usize {
        self.history.len()
    }
}

// ---------------------------------------------------------------------------
// ModeOverlay widget
// ---------------------------------------------------------------------------

/// Widget that renders mode-specific keybinding hints as an overlay.
///
/// Displays a compact list of available keybindings for the current mode.
/// Renders at the given area, typically at the bottom of the screen.
///
/// # Example
///
/// ```ignore
/// let overlay = ModeOverlay::new(&mode_manager);
/// overlay.render(area, frame);
/// ```
pub struct ModeOverlay<'a> {
    /// Reference to the mode manager.
    manager: &'a ModeManager,
    /// Whether to show the overlay at all.
    visible: bool,
    /// Style for the hint key labels.
    key_style: Style,
    /// Style for the hint description text.
    desc_style: Style,
}

impl<'a> ModeOverlay<'a> {
    /// Create a new mode overlay.
    #[must_use]
    pub fn new(manager: &'a ModeManager) -> Self {
        Self {
            manager,
            visible: true,
            key_style: Style::new().bold(),
            desc_style: Style::new().dim(),
        }
    }

    /// Set whether the overlay is visible.
    #[must_use]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set the key label style.
    #[must_use]
    pub fn key_style(mut self, style: Style) -> Self {
        self.key_style = style;
        self
    }

    /// Set the description style.
    #[must_use]
    pub fn desc_style(mut self, style: Style) -> Self {
        self.desc_style = style;
        self
    }
}

impl Widget for ModeOverlay<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if !self.visible || area.is_empty() {
            return;
        }

        let deg = frame.buffer.degradation;
        if !deg.render_content() {
            return;
        }

        let mode = self.manager.current();
        let hints = mode.key_hints();

        if hints.is_empty() {
            return;
        }

        let mut x = area.x;
        let y = area.y;
        let max_x = area.right();
        let key_style = if deg.apply_styling() { self.key_style } else { Style::default() };
        let desc_style = if deg.apply_styling() { self.desc_style } else { Style::default() };

        for (i, (key, desc)) in hints.iter().enumerate() {
            if x >= max_x {
                break;
            }

            // Draw key label
            let key_label = format!("{key} ");
            x = draw_text_span(frame, x, y, &key_label, key_style, max_x);

            // Draw description
            let desc_label = format!("{desc}");
            x = draw_text_span(frame, x, y, &desc_label, desc_style, max_x);

            // Separator between hints
            if i + 1 < hints.len() {
                x = draw_text_span(frame, x, y, "  ", desc_style, max_x);
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
    use ftui_core::event::{KeyCode, KeyEvent, Modifiers};

    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent {
            code: key,
            modifiers: Modifiers::NONE,
            kind: KeyEventKind::Press,
        }
    }

    // ── InteractionMode tests ──────────────────────────────────────

    #[test]
    fn mode_default_is_normal() {
        assert_eq!(InteractionMode::default(), InteractionMode::Normal);
    }

    #[test]
    fn mode_labels() {
        assert_eq!(InteractionMode::Normal.label(), "NORMAL");
        assert_eq!(InteractionMode::Copy.label(), "COPY");
        assert_eq!(InteractionMode::Command.label(), "CMD");
        assert_eq!(InteractionMode::VisualSelect.label(), "VISUAL");
        assert_eq!(InteractionMode::Insert.label(), "INSERT");
    }

    #[test]
    fn mode_locks_events() {
        assert!(!InteractionMode::Normal.locks_events());
        assert!(InteractionMode::Copy.locks_events());
        assert!(InteractionMode::Command.locks_events());
        assert!(InteractionMode::VisualSelect.locks_events());
        assert!(!InteractionMode::Insert.locks_events());
    }

    #[test]
    fn mode_key_hints_not_empty() {
        assert!(!InteractionMode::Normal.key_hints().is_empty());
        assert!(!InteractionMode::Copy.key_hints().is_empty());
    }

    #[test]
    fn mode_indicator_style_returns_style() {
        let style = InteractionMode::Normal.indicator_style();
        assert!(style.fg.is_some());
    }

    #[test]
    fn mode_debug_format() {
        let d = format!("{:?}", InteractionMode::Copy);
        assert!(d.contains("Copy"));
    }

    #[test]
    fn mode_copy_eq() {
        assert_eq!(InteractionMode::Copy, InteractionMode::Copy);
        assert_ne!(InteractionMode::Copy, InteractionMode::Normal);
    }

    // ── ModeManager tests ──────────────────────────────────────────

    #[test]
    fn manager_new_is_normal() {
        let mm = ModeManager::new();
        assert_eq!(mm.current(), InteractionMode::Normal);
    }

    #[test]
    fn manager_transition_changes_mode() {
        let mut mm = ModeManager::new();
        mm.transition(InteractionMode::Insert);
        assert_eq!(mm.current(), InteractionMode::Insert);
    }

    #[test]
    fn manager_transition_same_mode_is_noop() {
        let mut mm = ModeManager::new();
        mm.transition(InteractionMode::Normal);
        assert_eq!(mm.current(), InteractionMode::Normal);
        assert_eq!(mm.history_depth(), 0);
    }

    #[test]
    fn manager_escape_returns_to_normal() {
        let mut mm = ModeManager::new();
        mm.transition(InteractionMode::Copy);
        assert!(mm.handle_escape());
        assert_eq!(mm.current(), InteractionMode::Normal);
    }

    #[test]
    fn manager_escape_in_normal_is_noop() {
        let mut mm = ModeManager::new();
        assert!(!mm.handle_escape());
        assert_eq!(mm.current(), InteractionMode::Normal);
    }

    #[test]
    fn manager_escape_restores_history() {
        let mut mm = ModeManager::new();
        mm.transition(InteractionMode::Insert);
        mm.transition(InteractionMode::Command);
        assert_eq!(mm.current(), InteractionMode::Command);
        assert!(mm.handle_escape());
        assert_eq!(mm.current(), InteractionMode::Insert);
    }

    #[test]
    fn manager_locked_prevents_transition() {
        let mut mm = ModeManager::new();
        mm.set_locked(true);
        mm.transition(InteractionMode::Copy);
        assert_eq!(mm.current(), InteractionMode::Normal);
    }

    #[test]
    fn manager_locked_prevents_escape() {
        let mut mm = ModeManager::new();
        mm.transition(InteractionMode::Copy);
        mm.set_locked(true);
        assert!(!mm.handle_escape());
        assert_eq!(mm.current(), InteractionMode::Copy);
    }

    #[test]
    fn manager_route_key_esc_handled() {
        let mut mm = ModeManager::new();
        mm.transition(InteractionMode::Insert);
        let result = mm.route_key(&press(KeyCode::Escape));
        // Esc should transition back — after routing, mode is Normal
        assert_eq!(result, KeyAction::Transition(InteractionMode::Normal));
        assert_eq!(mm.current(), InteractionMode::Normal);
    }

    #[test]
    fn manager_route_key_i_in_normal_enters_insert() {
        let mut mm = ModeManager::new();
        let result = mm.route_key(&press(KeyCode::Char('i')));
        assert_eq!(result, KeyAction::Transition(InteractionMode::Insert));
        assert_eq!(mm.current(), InteractionMode::Insert);
    }

    #[test]
    fn manager_route_key_v_enters_visual() {
        let mut mm = ModeManager::new();
        let result = mm.route_key(&press(KeyCode::Char('v')));
        assert_eq!(result, KeyAction::Transition(InteractionMode::VisualSelect));
    }

    #[test]
    fn manager_route_key_colon_enters_command() {
        let mut mm = ModeManager::new();
        let result = mm.route_key(&press(KeyCode::Char(':')));
        assert_eq!(result, KeyAction::Transition(InteractionMode::Command));
    }

    #[test]
    fn manager_route_key_unknown_is_pass_through() {
        let mut mm = ModeManager::new();
        let result = mm.route_key(&press(KeyCode::Char('a')));
        assert_eq!(result, KeyAction::PassThrough);
    }

    #[test]
    fn manager_route_key_release_is_pass_through() {
        let mut mm = ModeManager::new();
        let event = KeyEvent {
            code: KeyCode::Char('i'),
            modifiers: Modifiers::NONE,
            kind: KeyEventKind::Release,
        };
        let result = mm.route_key(&event);
        assert_eq!(result, KeyAction::PassThrough);
    }

    #[test]
    fn manager_indicator_label() {
        let mm = ModeManager::new();
        assert_eq!(mm.indicator_label(), "[NORMAL]");
    }

    #[test]
    fn manager_reset_clears_mode_and_history() {
        let mut mm = ModeManager::new();
        mm.transition(InteractionMode::Copy);
        mm.transition(InteractionMode::VisualSelect);
        mm.reset();
        assert_eq!(mm.current(), InteractionMode::Normal);
        assert_eq!(mm.history_depth(), 0);
    }

    #[test]
    fn manager_default_trait() {
        let mm = ModeManager::default();
        assert_eq!(mm.current(), InteractionMode::Normal);
    }

    #[test]
    fn manager_bind_custom() {
        let mut mm = ModeManager::new();
        mm.bind(InteractionMode::Normal, KeyCode::Char('x'), KeyAction::Consumed);
        let result = mm.route_key(&press(KeyCode::Char('x')));
        assert_eq!(result, KeyAction::Consumed);
    }

    #[test]
    fn manager_unbind_removes_binding() {
        let mut mm = ModeManager::new();
        mm.unbind(InteractionMode::Normal, KeyCode::Char('i'));
        let result = mm.route_key(&press(KeyCode::Char('i')));
        assert_eq!(result, KeyAction::PassThrough);
    }

    #[test]
    fn manager_debug_format() {
        let mm = ModeManager::new();
        let d = format!("{mm:?}");
        assert!(d.contains("Normal"));
    }

    // ── ModeOverlay tests ──────────────────────────────────────────

    #[test]
    fn overlay_new_defaults() {
        let mm = ModeManager::new();
        let overlay = ModeOverlay::new(&mm);
        assert!(overlay.visible);
    }

    #[test]
    fn overlay_visible_setter() {
        let mm = ModeManager::new();
        let overlay = ModeOverlay::new(&mm).visible(false);
        assert!(!overlay.visible);
    }

    #[test]
    fn overlay_not_essential() {
        let mm = ModeManager::new();
        let overlay = ModeOverlay::new(&mm);
        assert!(!overlay.is_essential());
    }

    #[test]
    fn overlay_render_empty_area_no_panic() {
        let mm = ModeManager::new();
        let overlay = ModeOverlay::new(&mm);
        let mut pool = ftui_render::grapheme_pool::GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        overlay.render(Rect::new(0, 0, 0, 0), &mut frame);
    }
}
