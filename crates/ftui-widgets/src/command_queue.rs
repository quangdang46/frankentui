#![forbid(unsafe_code)]

//! Queued command indicator widget and backing command queue data structure.
//!
//! Provides a pill-shaped status widget that shows command execution
//! progress. The widget has zero height when the queue is empty, and
//! renders a compact status line with an interactive "[send]" label
//! when commands are queued or in flight.
//!
//! # Hit Regions
//!
//! The "[send]" label area registers a hit region so that the consumer
//! can detect clicks to trigger queued command dispatch. The region
//! carries the total pending command count as [`HitData`].
//!
//! [`HitData`]: ftui_render::frame::HitData

use crate::{Widget, apply_style, draw_text_span};
use ftui_core::geometry::Rect;
use ftui_render::cell::Cell;
use ftui_render::frame::{Frame, HitId, HitRegion};
use ftui_style::Style;
use ftui_text::display_width;
use std::collections::VecDeque;
use web_time::Instant;

// ---------------------------------------------------------------------------
// Hit region constant
// ---------------------------------------------------------------------------

/// Hit region tag for the `[send]` label in [`QueuedCommandIndicator`].
///
/// Consumers can compare this against the region returned by
/// `frame.hit_test` / `hit_test_detailed` to recognise clicks on the
/// send label.
pub const CMD_QUEUE_HIT_SEND: HitRegion = HitRegion::Custom(4);

// ---------------------------------------------------------------------------
// CommandStatus
// ---------------------------------------------------------------------------

/// Execution status of a single queued command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStatus {
    /// Command is waiting in the queue.
    Pending,
    /// Command is currently being executed.
    Running,
    /// Command finished successfully.
    Completed,
    /// Command finished with an error.
    Failed,
}

// ---------------------------------------------------------------------------
// QueuedCommand
// ---------------------------------------------------------------------------

/// A single command entry tracked by the [`CommandQueue`].
#[derive(Debug, Clone)]
pub struct QueuedCommand {
    /// Unique command identifier.
    pub id: u64,
    /// Human-readable description of the command.
    pub description: String,
    /// Current execution status.
    pub status: CommandStatus,
    /// Instant when the command was enqueued.
    pub created_at: Instant,
    /// Instant when execution started, if known.
    pub started_at: Option<Instant>,
    /// Instant when execution finished, if known.
    pub completed_at: Option<Instant>,
}

// ---------------------------------------------------------------------------
// CommandQueue
// ---------------------------------------------------------------------------

/// Default maximum number of commands retained in the queue history.
const DEFAULT_MAX_HISTORY: usize = 100;

/// Manages a FIFO queue of commands with execution status tracking.
///
/// Commands are enqueued with a unique incrementing ID and transition
/// through `Pending -> Running -> Completed | Failed`. The queue
/// automatically drops the oldest completed / failed entries when the
/// total exceeds [`max_history`] (default 100), preserving pending and
/// running commands.
#[derive(Debug, Clone)]
pub struct CommandQueue {
    commands: VecDeque<QueuedCommand>,
    max_history: usize,
    next_id: u64,
}

impl CommandQueue {
    /// Create a new empty queue with the default history limit (100).
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: VecDeque::new(),
            max_history: DEFAULT_MAX_HISTORY,
            next_id: 1,
        }
    }

    /// Create a new empty queue with the default history limit.
    ///
    /// Equivalent to [`new`](Self::new), provided for API symmetry with
    /// other builders in the crate.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new()
    }

    /// Add a command to the back of the queue.
    ///
    /// If the queue has reached [`max_history`] entries, the oldest
    /// completed or failed command is dropped to make room. Pending and
    /// running commands are never evicted — the queue may temporarily
    /// exceed its limit if every entry is still in flight.
    ///
    /// Returns the unique ID assigned to the new command.
    pub fn enqueue(&mut self, description: impl Into<String>) -> u64 {
        // Evict the oldest completed / failed entries when at capacity.
        while self.commands.len() >= self.max_history {
            let front_is_expendable = self
                .commands
                .front()
                .is_some_and(|cmd| matches!(cmd.status, CommandStatus::Completed | CommandStatus::Failed));
            if front_is_expendable {
                self.commands.pop_front();
            } else {
                break;
            }
        }

        let id = self.next_id;
        self.next_id += 1;

        self.commands.push_back(QueuedCommand {
            id,
            description: description.into(),
            status: CommandStatus::Pending,
            created_at: Instant::now(),
            started_at: None,
            completed_at: None,
        });

        id
    }

    /// Mark the command with the given ID as running.
    ///
    /// This is a no-op if the ID does not exist in the queue.
    pub fn mark_running(&mut self, id: u64) {
        if let Some(cmd) = self.find_mut(id) {
            cmd.status = CommandStatus::Running;
            cmd.started_at = Some(Instant::now());
        }
    }

    /// Mark the command with the given ID as completed.
    ///
    /// This is a no-op if the ID does not exist in the queue.
    pub fn mark_completed(&mut self, id: u64) {
        if let Some(cmd) = self.find_mut(id) {
            cmd.status = CommandStatus::Completed;
            cmd.completed_at = Some(Instant::now());
        }
    }

    /// Mark the command with the given ID as failed.
    ///
    /// This is a no-op if the ID does not exist in the queue.
    pub fn mark_failed(&mut self, id: u64) {
        if let Some(cmd) = self.find_mut(id) {
            cmd.status = CommandStatus::Failed;
            cmd.completed_at = Some(Instant::now());
        }
    }

    /// Number of pending (queued but not yet running) commands.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|cmd| cmd.status == CommandStatus::Pending)
            .count()
    }

    /// Number of currently executing commands.
    #[must_use]
    pub fn running_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|cmd| cmd.status == CommandStatus::Running)
            .count()
    }

    /// Number of successfully completed commands.
    #[must_use]
    pub fn completed_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|cmd| cmd.status == CommandStatus::Completed)
            .count()
    }

    /// Number of commands that finished with an error.
    #[must_use]
    pub fn failed_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|cmd| cmd.status == CommandStatus::Failed)
            .count()
    }

    /// Total number of commands currently tracked in the queue (all statuses).
    #[must_use]
    pub fn total_count(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` when there are zero commands in the queue.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Iterate over all queued commands from oldest to newest.
    #[must_use]
    pub fn commands(&self) -> impl Iterator<Item = &QueuedCommand> {
        self.commands.iter()
    }

    /// Return the IDs of all pending (not yet running) commands.
    #[must_use]
    pub fn pending(&self) -> Vec<u64> {
        self.commands
            .iter()
            .filter(|cmd| cmd.status == CommandStatus::Pending)
            .map(|cmd| cmd.id)
            .collect()
    }

    /// Return a `(done, total)` pair for display purposes.
    ///
    /// `done` is the number of commands that have finished (either
    /// completed or failed). `total` is the total number of commands
    /// tracked in the queue.
    #[must_use]
    pub fn progress(&self) -> (usize, usize) {
        let done = self.completed_count() + self.failed_count();
        let total = self.commands.len();
        (done, total)
    }

    /// Remove all completed and failed commands from the queue.
    ///
    /// Pending and running commands are preserved.
    pub fn clear_completed(&mut self) {
        self.commands.retain(|cmd| {
            !matches!(cmd.status, CommandStatus::Completed | CommandStatus::Failed)
        });
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn find_mut(&mut self, id: u64) -> Option<&mut QueuedCommand> {
        self.commands.iter_mut().find(|cmd| cmd.id == id)
    }
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// QueuedCommandIndicator
// ---------------------------------------------------------------------------

/// A pill-shaped widget that renders the current state of a
/// [`CommandQueue`].
///
/// # Rendering
///
/// When the queue is empty the widget renders nothing and occupies zero
/// visual height (the [`render`](Widget::render) method returns without
/// drawing anything).
///
/// When commands are queued but none are executing:
///
/// ```text
/// [send] · {N} queued · Tab to send next
/// ```
///
/// While one or more commands are running:
///
/// ```text
/// ⏳ {done}/{total} commands completed
/// ```
///
/// # Hit Testing
///
/// The `[send]` label registers a hit region
/// ([`CMD_QUEUE_HIT_SEND`]) so that mouse clicks can be routed to
/// dispatch queued commands.
///
/// # Degradation
///
/// The widget respects the frame's [`DegradationLevel`]. At
/// `Skeleton` and below the widget skips rendering entirely. At
/// `NoStyling` the configured styles are ignored in favour of the
/// terminal defaults.
///
/// [`DegradationLevel`]: ftui_render::budget::DegradationLevel
#[derive(Debug, Clone)]
pub struct QueuedCommandIndicator<'a> {
    /// Shared reference to the command queue being visualised.
    queue: &'a CommandQueue,
    /// Whether to render detailed progress during execution.
    show_progress: bool,
    /// Style applied to the pill background.
    style: Style,
    /// Style applied to body text.
    text_style: Style,
    /// Style applied to the highlighted `[send]` label.
    send_style: Style,
}

impl<'a> QueuedCommandIndicator<'a> {
    /// Create a new indicator that reads from the given queue.
    #[must_use]
    pub fn new(queue: &'a CommandQueue) -> Self {
        Self {
            queue,
            show_progress: true,
            style: Style::default(),
            text_style: Style::default(),
            send_style: Style::default(),
        }
    }

    /// Control whether detailed progress is shown during execution.
    ///
    /// When `true` (the default), the widget shows
    /// `"⏳ {done}/{total} commands completed"` while commands are
    /// running. When `false` the idle message is shown regardless of
    /// execution state.
    #[must_use]
    pub fn show_progress(mut self, show_progress: bool) -> Self {
        self.show_progress = show_progress;
        self
    }

    /// Set the background / base style for the pill area.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the text style for body labels.
    #[must_use]
    pub fn text_style(mut self, text_style: Style) -> Self {
        self.text_style = text_style;
        self
    }

    /// Set the style for the highlighted `[send]` label.
    #[must_use]
    pub fn send_style(mut self, send_style: Style) -> Self {
        self.send_style = send_style;
        self
    }

    /// Resolve the effective background style based on degradation.
    fn effective_style(&self, deg: &ftui_render::budget::DegradationLevel) -> Style {
        if deg.apply_styling() {
            self.style
        } else {
            Style::default()
        }
    }

    /// Resolve the effective text style based on degradation.
    fn effective_text_style(&self, deg: &ftui_render::budget::DegradationLevel) -> Style {
        if deg.apply_styling() {
            self.text_style
        } else {
            Style::default()
        }
    }

    /// Resolve the effective send-label style based on degradation.
    fn effective_send_style(&self, deg: &ftui_render::budget::DegradationLevel) -> Style {
        if deg.apply_styling() {
            self.send_style
        } else {
            Style::default()
        }
    }
}

impl Widget for QueuedCommandIndicator<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        // Zero height when empty: render nothing, no clear.
        if self.queue.is_empty() || area.height == 0 {
            return;
        }

        let deg = frame.buffer.degradation;

        // Skeleton / below: skip entirely.
        if !deg.render_content() {
            return;
        }

        let style = self.effective_style(&deg);
        let text_style = self.effective_text_style(&deg);
        let send_style = self.effective_send_style(&deg);

        // Fill the full area with the background style.
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let mut cell = Cell::from_char(' ');
                apply_style(&mut cell, style);
                frame.buffer.set_fast(x, y, cell);
            }
        }

        // Calculate layout for the single-row indicator.
        let y = area.y;
        let max_x = area.right();
        let mut x = area.x;

        // ── [send] label ────────────────────────────────────────────
        let send_text = "[send]";
        let send_w = display_width(send_text) as u16;

        x = draw_text_span(frame, x, y, send_text, send_style, max_x);

        // Register hit region over the [send] label.
        let send_area = Rect::new(area.x, y, send_w, 1);
        let pending = self.queue.pending_count() as u64;
        frame.register_hit(send_area, HitId::new(0), CMD_QUEUE_HIT_SEND, pending);

        // ── separator ──────────────────────────────────────────────
        if x < max_x {
            x = draw_text_span(frame, x, y, " · ", text_style, max_x);
        }

        // ── status text ────────────────────────────────────────────
        let running = self.queue.running_count();
        let completed = self.queue.completed_count();
        let has_active_work = (running + completed) > 0;
        let show_execution = has_active_work && self.show_progress;

        let status_text = if show_execution {
            let (done, total) = self.queue.progress();
            format!("{done}/{total} commands completed")
        } else {
            let n = self.queue.pending_count();
            format!("{n} queued · Tab to send next")
        };

        // Prepend spinner during execution.
        if show_execution && x < max_x {
            x = draw_text_span(frame, x, y, "\u{23F3} ", text_style, max_x);
        }

        if x < max_x {
            let _ = draw_text_span(frame, x, y, &status_text, text_style, max_x);
        }

        // Reduced motion is implicitly respected: this widget performs
        // no animation regardless of the preference.
    }

    fn is_essential(&self) -> bool {
        // Command status is essential user-facing information.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::budget::DegradationLevel;
    use ftui_render::cell::PackedRgba;
    use ftui_render::grapheme_pool::GraphemePool;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn row_chars(frame: &ftui_render::frame::Frame, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| {
                frame
                    .buffer
                    .get(x, y)
                    .and_then(|c| c.content.as_char())
                    .unwrap_or(' ')
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // CommandQueue — construction
    // -----------------------------------------------------------------------

    #[test]
    fn new_queue_is_empty() {
        let q = CommandQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.total_count(), 0);
    }

    #[test]
    fn with_defaults_is_empty() {
        let q = CommandQueue::with_defaults();
        assert!(q.is_empty());
    }

    #[test]
    fn default_is_empty() {
        let q = CommandQueue::default();
        assert!(q.is_empty());
    }

    // -----------------------------------------------------------------------
    // CommandQueue — enqueue
    // -----------------------------------------------------------------------

    #[test]
    fn enqueue_adds_command() {
        let mut q = CommandQueue::new();
        let id = q.enqueue("hello");
        assert_eq!(id, 1);
        assert!(!q.is_empty());
        assert_eq!(q.total_count(), 1);
    }

    #[test]
    fn enqueue_returns_incrementing_ids() {
        let mut q = CommandQueue::new();
        assert_eq!(q.enqueue("a"), 1);
        assert_eq!(q.enqueue("b"), 2);
        assert_eq!(q.enqueue("c"), 3);
    }

    #[test]
    fn enqueue_default_status_is_pending() {
        let mut q = CommandQueue::new();
        let id = q.enqueue("test");
        q.commands()
            .find(|cmd| cmd.id == id)
            .map(|cmd| assert_eq!(cmd.status, CommandStatus::Pending));
    }

    #[test]
    fn enqueue_with_string_works() {
        let mut q = CommandQueue::new();
        let id = q.enqueue(String::from("from string"));
        assert_eq!(id, 1);
    }

    // -----------------------------------------------------------------------
    // CommandQueue — status transitions
    // -----------------------------------------------------------------------

    #[test]
    fn mark_running_changes_status() {
        let mut q = CommandQueue::new();
        let id = q.enqueue("cmd");
        q.mark_running(id);
        assert_eq!(q.running_count(), 1);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn mark_completed_changes_status() {
        let mut q = CommandQueue::new();
        let id = q.enqueue("cmd");
        q.mark_running(id);
        q.mark_completed(id);
        assert_eq!(q.completed_count(), 1);
        assert_eq!(q.running_count(), 0);
    }

    #[test]
    fn mark_failed_changes_status() {
        let mut q = CommandQueue::new();
        let id = q.enqueue("cmd");
        q.mark_running(id);
        q.mark_failed(id);
        assert_eq!(q.failed_count(), 1);
        assert_eq!(q.running_count(), 0);
    }

    #[test]
    fn mark_unknown_id_is_noop() {
        let mut q = CommandQueue::new();
        q.mark_running(999);
        q.mark_completed(999);
        q.mark_failed(999);
        assert!(q.is_empty());
    }

    // -----------------------------------------------------------------------
    // CommandQueue — counts
    // -----------------------------------------------------------------------

    #[test]
    fn counts_reflect_mixed_statuses() {
        let mut q = CommandQueue::new();
        let a = q.enqueue("a"); // pending -> running -> completed
        let b = q.enqueue("b"); // pending -> running
        let _c = q.enqueue("c"); // pending

        q.mark_running(a);
        q.mark_completed(a);
        q.mark_running(b);

        assert_eq!(q.pending_count(), 1); // c
        assert_eq!(q.running_count(), 1); // b
        assert_eq!(q.completed_count(), 1); // a
        assert_eq!(q.failed_count(), 0);
        assert_eq!(q.total_count(), 3);
    }

    // -----------------------------------------------------------------------
    // CommandQueue — pending
    // -----------------------------------------------------------------------

    #[test]
    fn pending_returns_only_pending_ids() {
        let mut q = CommandQueue::new();
        let a = q.enqueue("a");
        let b = q.enqueue("b");
        let c = q.enqueue("c");

        q.mark_running(b);

        let mut ids = q.pending();
        ids.sort();
        assert_eq!(ids, vec![a, c]);
    }

    // -----------------------------------------------------------------------
    // CommandQueue — progress
    // -----------------------------------------------------------------------

    #[test]
    fn progress_returns_done_and_total() {
        let mut q = CommandQueue::new();
        let a = q.enqueue("a");
        let _b = q.enqueue("b");
        let c = q.enqueue("c");

        q.mark_running(a);
        q.mark_completed(a);
        q.mark_running(c);
        q.mark_failed(c);

        let (done, total) = q.progress();
        assert_eq!(done, 2); // a (completed) + c (failed)
        assert_eq!(total, 3);
    }

    #[test]
    fn progress_empty_queue() {
        let q = CommandQueue::new();
        assert_eq!(q.progress(), (0, 0));
    }

    // -----------------------------------------------------------------------
    // CommandQueue — clear_completed
    // -----------------------------------------------------------------------

    #[test]
    fn clear_completed_removes_completed_and_failed() {
        let mut q = CommandQueue::new();
        let a = q.enqueue("a");
        let _b = q.enqueue("b");
        let c = q.enqueue("c");

        q.mark_running(a);
        q.mark_completed(a);
        q.mark_running(c);
        q.mark_failed(c);

        q.clear_completed();

        assert_eq!(q.total_count(), 1);
        assert_eq!(q.pending_count(), 1); // b remains pending
    }

    #[test]
    fn clear_completed_preserves_pending_and_running() {
        let mut q = CommandQueue::new();
        q.enqueue("pending");
        let running = q.enqueue("running");
        q.mark_running(running);

        q.clear_completed();

        assert_eq!(q.total_count(), 2);
    }

    #[test]
    fn clear_completed_empty_is_noop() {
        let mut q = CommandQueue::new();
        q.clear_completed();
        assert!(q.is_empty());
    }

    // -----------------------------------------------------------------------
    // CommandQueue — max_history eviction
    // -----------------------------------------------------------------------

    #[test]
    fn enqueue_evicts_oldest_completed() {
        let mut q = CommandQueue::new();
        q.max_history = 2;

        let a = q.enqueue("a");
        let b = q.enqueue("b");

        // Mark "a" as completed so it becomes evictable.
        q.mark_running(a);
        q.mark_completed(a);

        // Enqueue "c" — should evict "a".
        let c = q.enqueue("c");

        let ids: Vec<u64> = q.commands().map(|c| c.id).collect();
        assert_eq!(ids, vec![b, c]);
    }

    #[test]
    fn enqueue_evicts_oldest_failed() {
        let mut q = CommandQueue::new();
        q.max_history = 2;

        let a = q.enqueue("a");
        let b = q.enqueue("b");

        q.mark_running(a);
        q.mark_failed(a);

        q.enqueue("c");

        let ids: Vec<u64> = q.commands().map(|c| c.id).collect();
        assert_eq!(ids, vec![b, 3]);
    }

    #[test]
    fn enqueue_does_not_evict_pending() {
        let mut q = CommandQueue::new();
        q.max_history = 1;

        q.enqueue("a"); // pending, not evictable
        q.enqueue("b"); // cannot evict "a", so queue grows past limit

        assert_eq!(q.total_count(), 2);
    }

    #[test]
    fn enqueue_does_not_evict_running() {
        let mut q = CommandQueue::new();
        q.max_history = 1;

        let a = q.enqueue("a");
        q.mark_running(a);
        q.enqueue("b");

        assert_eq!(q.total_count(), 2);
    }

    // -----------------------------------------------------------------------
    // CommandQueue — commands iterator
    // -----------------------------------------------------------------------

    #[test]
    fn commands_yields_fifo_order() {
        let mut q = CommandQueue::new();
        q.enqueue("first");
        q.enqueue("second");
        q.enqueue("third");

        let descs: Vec<&str> = q.commands().map(|c| c.description.as_str()).collect();
        assert_eq!(descs, vec!["first", "second", "third"]);
    }

    #[test]
    fn commands_empty_queue_yields_nothing() {
        let q = CommandQueue::new();
        assert_eq!(q.commands().count(), 0);
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — empty queue
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_empty_queue_renders_nothing() {
        let q = CommandQueue::new();
        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 30, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(30, 1, &mut pool);

        indicator.render(area, &mut frame);

        // All cells should be empty (no clear either).
        let row = row_chars(&frame, 0, 30);
        assert!(row.chars().all(|c| c == ' '));
    }

    #[test]
    fn indicator_zero_height_renders_nothing() {
        let mut q = CommandQueue::new();
        q.enqueue("test");
        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 30, 0);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(30, 1, &mut pool);

        indicator.render(area, &mut frame);

        // If the area has zero height, nothing should have been drawn.
        // Buffer at y=0 should still be empty.
        let row = row_chars(&frame, 0, 30);
        assert!(row.chars().all(|c| c == ' '));
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — idle state
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_idle_shows_send_and_count() {
        let mut q = CommandQueue::new();
        q.enqueue("cmd1");
        q.enqueue("cmd2");

        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        let row = row_chars(&frame, 0, 40).trim_end().to_string();
        assert!(row.contains("[send]"), "Row: {row:?}");
        assert!(row.contains("2 queued"), "Row: {row:?}");
        assert!(row.contains("Tab to send next"), "Row: {row:?}");
    }

    #[test]
    fn indicator_idle_single_command() {
        let mut q = CommandQueue::new();
        q.enqueue("only");

        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        let row = row_chars(&frame, 0, 40);
        assert!(row.contains("1 queued"), "Row: {row:?}");
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — execution state
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_execution_shows_progress() {
        let mut q = CommandQueue::new();
        let a = q.enqueue("a");
        q.enqueue("b");
        q.mark_running(a);

        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        let row = row_chars(&frame, 0, 40);
        assert!(row.contains("0/2"), "Row: {row:?}");
        assert!(row.contains("commands completed"), "Row: {row:?}");
    }

    #[test]
    fn indicator_execution_with_completions() {
        let mut q = CommandQueue::new();
        let a = q.enqueue("a");
        let b = q.enqueue("b");
        let c = q.enqueue("c");
        q.mark_running(a);
        q.mark_completed(a);
        q.mark_running(b);
        q.mark_completed(b);
        q.mark_running(c);

        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        let row = row_chars(&frame, 0, 40);
        assert!(row.contains("2/3"), "Row: {row:?}");
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — show_progress toggle
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_show_progress_false_shows_idle_message() {
        let mut q = CommandQueue::new();
        let a = q.enqueue("a");
        q.enqueue("b");
        q.mark_running(a);

        let indicator = QueuedCommandIndicator::new(&q).show_progress(false);
        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        let row = row_chars(&frame, 0, 40);
        assert!(row.contains("Tab to send next"), "Row: {row:?}");
        assert!(!row.contains("commands completed"), "Row: {row:?}");
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — hit region
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_registers_hit_on_send_label() {
        let mut q = CommandQueue::new();
        q.enqueue("a");
        q.enqueue("b");
        q.enqueue("c");

        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        // Hit-test on the [send] area (should be at x=0).
        let hit = frame.hit_test(0, 0);
        assert!(hit.is_some(), "Expected a hit on [send]");

        if let Some((_id, region, data)) = hit {
            assert_eq!(region, CMD_QUEUE_HIT_SEND);
            assert_eq!(data, 3); // 3 pending commands
        }
    }

    #[test]
    fn indicator_hit_data_reflects_pending_count() {
        let mut q = CommandQueue::new();
        q.enqueue("a");
        q.enqueue("b");

        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        let hit = frame.hit_test(0, 0);
        assert!(hit.is_some());
        if let Some((_id, _region, data)) = hit {
            assert_eq!(data, 2);
        }
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — style application
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_applies_styles() {
        let fg = PackedRgba::rgb(200, 100, 50);
        let bg = PackedRgba::rgb(10, 20, 30);
        let send_fg = PackedRgba::rgb(0, 255, 0);

        let mut q = CommandQueue::new();
        q.enqueue("test");

        let indicator = QueuedCommandIndicator::new(&q)
            .style(Style::new().bg(bg))
            .text_style(Style::new().fg(fg))
            .send_style(Style::new().fg(send_fg));

        let area = Rect::new(0, 0, 40, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 1, &mut pool);

        indicator.render(area, &mut frame);

        // [send] should use send_style
        assert_eq!(frame.buffer.get(0, 0).unwrap().fg, send_fg);
        // The separator area after [send] should use text_style
        let sep_x = "[send]".len() as u16;
        assert_eq!(frame.buffer.get(sep_x, 0).unwrap().fg, fg);
        // Background should be applied
        assert_eq!(frame.buffer.get(0, 0).unwrap().bg, bg);
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — degradation
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_skeleton_skips_rendering() {
        let mut q = CommandQueue::new();
        q.enqueue("test");

        let indicator = QueuedCommandIndicator::new(&q);
        let area = Rect::new(0, 0, 30, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(30, 1, &mut pool);
        frame.buffer.degradation = DegradationLevel::Skeleton;

        indicator.render(area, &mut frame);

        let row = row_chars(&frame, 0, 30);
        assert!(row.chars().all(|c| c == ' '));
    }

    #[test]
    fn indicator_no_styling_drops_colors() {
        let fg = PackedRgba::rgb(200, 100, 50);
        let mut q = CommandQueue::new();
        q.enqueue("test");

        let indicator = QueuedCommandIndicator::new(&q)
            .style(Style::new().bg(PackedRgba::rgb(1, 2, 3)))
            .text_style(Style::new().fg(fg))
            .send_style(Style::new().fg(PackedRgba::rgb(4, 5, 6)));

        let area = Rect::new(0, 0, 30, 1);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(30, 1, &mut pool);
        frame.buffer.degradation = DegradationLevel::NoStyling;

        indicator.render(area, &mut frame);

        // At NoStyling, styles are replaced with defaults.
        // Default fg should be applied.
        let cell = frame.buffer.get(0, 0).unwrap();
        assert_ne!(cell.fg, fg, "Should not use configured style at NoStyling");
    }

    // -----------------------------------------------------------------------
    // QueuedCommandIndicator — is_essential
    // -----------------------------------------------------------------------

    #[test]
    fn indicator_is_essential() {
        let q = CommandQueue::new();
        let indicator = QueuedCommandIndicator::new(&q);
        assert!(indicator.is_essential());
    }
}
