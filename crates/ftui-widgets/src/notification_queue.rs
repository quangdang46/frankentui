#![forbid(unsafe_code)]

//! Notification queue manager for handling multiple concurrent toast notifications.
//!
//! The queue system provides:
//! - FIFO ordering with priority support (Urgent notifications jump ahead)
//! - Maximum visible limit with automatic stacking
//! - Content-based deduplication within a configurable time window
//! - Automatic expiry processing via tick-based updates
//! - Notification history with bounded retention
//! - Click-to-dismiss on individual toasts
//!
//! # Example
//!
//! ```ignore
//! let mut queue = NotificationQueue::new(QueueConfig::default());
//!
//! // Push notifications
//! queue.push(Toast::new("File saved").icon(ToastIcon::Success), NotificationPriority::Normal);
//! queue.push(Toast::new("Error!").icon(ToastIcon::Error), NotificationPriority::Urgent);
//!
//! // Process in your event loop
//! let actions = queue.tick(Duration::from_millis(16));
//! for action in actions {
//!     match action {
//!         QueueAction::Show(toast) => { /* render toast */ }
//!         QueueAction::Hide(id) => { /* remove toast */ }
//!     }
//! }
//!
//! // Access notification history
//! for entry in queue.history().entries() {
//!     println!("Past toast: {} ({:?})", entry.message, entry.reason);
//! }
//! ```

use ahash::AHashMap;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use web_time::{Duration, Instant};

use ftui_core::geometry::Rect;
use ftui_render::frame::{Frame, HitId, HitRegion};

use crate::toast::{Toast, ToastIcon, ToastId, ToastPosition, ToastStyle};
use crate::Widget;
use ftui_style::Style;

/// Priority level for notifications.
///
/// Higher priority notifications are displayed sooner.
/// `Urgent` notifications jump to the front of the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum NotificationPriority {
    /// Low priority, displayed last.
    Low = 0,
    /// Normal priority (default).
    #[default]
    Normal = 1,
    /// High priority, displayed before Normal/Low.
    High = 2,
    /// Urgent priority, jumps to front immediately.
    Urgent = 3,
}

/// Configuration for the notification queue.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Maximum number of toasts visible at once.
    pub max_visible: usize,
    /// Maximum number of notifications waiting in queue.
    pub max_queued: usize,
    /// Default auto-dismiss duration.
    pub default_duration: Duration,
    /// Anchor position for the toast stack.
    pub position: ToastPosition,
    /// Vertical spacing between stacked toasts.
    pub stagger_offset: u16,
    /// Time window for deduplication (in ms).
    pub dedup_window_ms: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_visible: 5,
            max_queued: 10,
            default_duration: Duration::from_secs(5),
            position: ToastPosition::BottomRight,
            stagger_offset: 1,
            dedup_window_ms: 1000,
        }
    }
}

impl QueueConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum visible toasts.
    #[must_use]
    pub fn max_visible(mut self, max: usize) -> Self {
        self.max_visible = max;
        self
    }

    /// Set maximum queued notifications.
    #[must_use]
    pub fn max_queued(mut self, max: usize) -> Self {
        self.max_queued = max;
        self
    }

    /// Set default duration for auto-dismiss.
    #[must_use]
    pub fn default_duration(mut self, duration: Duration) -> Self {
        self.default_duration = duration;
        self
    }

    /// Set anchor position for the toast stack.
    #[must_use]
    pub fn position(mut self, position: ToastPosition) -> Self {
        self.position = position;
        self
    }

    /// Set vertical spacing between stacked toasts.
    #[must_use]
    pub fn stagger_offset(mut self, offset: u16) -> Self {
        self.stagger_offset = offset;
        self
    }

    /// Set deduplication time window in milliseconds.
    #[must_use]
    pub fn dedup_window_ms(mut self, ms: u64) -> Self {
        self.dedup_window_ms = ms;
        self
    }
}

/// Internal representation of a queued notification.
#[derive(Debug)]
struct QueuedNotification {
    toast: Toast,
    priority: NotificationPriority,
    /// When the notification was queued (for potential time-based priority decay).
    #[allow(dead_code)]
    created_at: Instant,
    content_hash: u64,
}

impl QueuedNotification {
    fn new(toast: Toast, priority: NotificationPriority) -> Self {
        let content_hash = Self::compute_hash(&toast);
        Self {
            toast,
            priority,
            created_at: Instant::now(),
            content_hash,
        }
    }

    fn compute_hash(toast: &Toast) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        toast.content.message.hash(&mut hasher);
        if let Some(ref title) = toast.content.title {
            title.hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// Actions returned by `tick()` to be processed by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueAction {
    /// Show a new toast at the given position.
    Show(ToastId),
    /// Hide an existing toast.
    Hide(ToastId),
    /// Reposition a toast (for stacking adjustments).
    Reposition(ToastId),
}

/// Why a notification was removed from the visible/pending queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DismissReason {
    /// User manually dismissed the toast.
    UserDismissed,
    /// Toast expired after its configured duration.
    AutoExpired,
    /// Toast was evicted to make room for a higher-priority notification.
    Evicted,
}

/// A single entry in the notification history.
///
/// Captures metadata about a dismissed or evicted toast for display
/// in a history panel.
#[derive(Debug, Clone)]
pub struct NotificationHistoryEntry {
    /// The toast's message text.
    pub message: String,
    /// Optional title.
    pub title: Option<String>,
    /// The icon that was displayed, if any.
    pub icon: Option<ToastIcon>,
    /// The style variant.
    pub style_variant: ToastStyle,
    /// Priority level when the toast was active.
    pub priority: NotificationPriority,
    /// When the toast was originally created.
    pub created_at: Instant,
    /// When the toast was dismissed/evicted.
    pub dismissed_at: Instant,
    /// Why this toast was removed from the active queue.
    pub reason: DismissReason,
    /// How long the toast was visible (before auto-dismiss or eviction).
    pub visible_duration: Duration,
}

/// Bounded history of past notifications.
///
/// Stores dismissed/evicted toast metadata so users can review
/// past notifications in a scrollable panel.
#[derive(Debug, Clone)]
pub struct NotificationHistory {
    /// Entries in insertion order (newest last).
    entries: VecDeque<NotificationHistoryEntry>,
    /// Maximum number of entries to retain.
    max_entries: usize,
}

impl Default for NotificationHistory {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries: 100,
        }
    }
}

impl NotificationHistory {
    /// Create a new notification history with the given capacity.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
        }
    }

    /// Get all history entries, newest first.
    pub fn entries(&self) -> impl Iterator<Item = &NotificationHistoryEntry> {
        self.entries.iter().rev()
    }

    /// Get the number of entries in the history.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all history entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get the maximum number of entries.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Add an entry to the history, evicting oldest if at capacity.
    fn push(&mut self, entry: NotificationHistoryEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }
}

/// Queue statistics for monitoring and debugging.
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    /// Total notifications pushed.
    pub total_pushed: u64,
    /// Notifications rejected due to queue overflow.
    pub overflow_count: u64,
    /// Notifications rejected due to deduplication.
    pub dedup_count: u64,
    /// Notifications dismissed by user.
    pub user_dismissed: u64,
    /// Notifications expired automatically.
    pub auto_expired: u64,
}

/// Notification queue manager.
///
/// Manages multiple toast notifications with priority ordering, deduplication,
/// and automatic expiry. Use `push` to add notifications and `tick` to process
/// expiry in your event loop.
///
/// The queue maintains a [`NotificationHistory`] of dismissed and evicted
/// toasts that can be displayed via the [`NotificationHistoryPanel`] widget.
#[derive(Debug)]
pub struct NotificationQueue {
    /// Pending notifications waiting to be displayed.
    queue: VecDeque<QueuedNotification>,
    /// Currently visible toasts.
    visible: Vec<Toast>,
    /// Configuration.
    config: QueueConfig,
    /// Deduplication window.
    dedup_window: Duration,
    /// Recent content hashes for deduplication.
    recent_hashes: AHashMap<u64, Instant>,
    /// Statistics.
    stats: QueueStats,
    /// History of dismissed/evicted toasts.
    history: NotificationHistory,
}

/// Hit region for a toast's dismiss area.
pub const TOAST_STACK_HIT_DISMISS: HitRegion = HitRegion::Custom(3);

/// Widget that renders the visible toasts in a queue.
///
/// This is a thin renderer over `NotificationQueue`, keeping stacking logic
/// centralized in the queue while ensuring the draw path stays deterministic.
///
/// Toasts are rendered with their animation offsets applied (slide/fade).
/// Each toast also registers a hit region so the application can implement
/// click-to-dismiss.
pub struct NotificationStack<'a> {
    queue: &'a NotificationQueue,
    margin: u16,
    /// Counter for hit IDs to ensure uniqueness per frame.
    /// Rust default does NOT provide this, so we track it via a counter.
    hit_counter: Option<u32>,
}

impl<'a> NotificationStack<'a> {
    /// Create a new notification stack renderer.
    pub fn new(queue: &'a NotificationQueue) -> Self {
        Self {
            queue,
            margin: 1,
            hit_counter: None,
        }
    }

    /// Set the margin from the screen edge.
    #[must_use]
    pub fn margin(mut self, margin: u16) -> Self {
        self.margin = margin;
        self
    }

    /// Enable mouse hit registration so individual toasts can be
    /// click-dismissed. Returns self with hit tracking enabled.
    #[must_use]
    pub fn with_hit_testing(mut self) -> Self {
        self.hit_counter = Some(0);
        self
    }
}

impl Widget for NotificationStack<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() || self.queue.visible().is_empty() {
            return;
        }

        let positions = self
            .queue
            .calculate_positions(area.width, area.height, self.margin);

        // We need a mutable copy for the hit counter
        let mut next_hit_id = self.hit_counter.map(|_| 0u32);

        for (toast, (_, rel_x, rel_y)) in self.queue.visible().iter().zip(positions.iter()) {
            let (toast_width, toast_height) = toast.calculate_dimensions();

            // Apply animation offset
            let (dx, dy) = toast.animation_offset();
            let raw_x = (area.x as i16).saturating_add(*rel_x as i16).saturating_add(dx);
            let raw_y = (area.y as i16).saturating_add(*rel_y as i16).saturating_add(dy);
            let x = raw_x.max(0).min(area.right() as i16 - 1) as u16;
            let y = raw_y.max(0).min(area.bottom() as i16 - 1) as u16;

            let toast_area = Rect::new(x, y, toast_width, toast_height);
            let render_area = toast_area.intersection(&area);
            if !render_area.is_empty() {
                toast.render(render_area, frame);

                // Register hit region for click-to-dismiss
                if let Some(counter) = &mut next_hit_id {
                    let id = HitId::new(*counter);
                    *counter += 1;
                    frame.register_hit(render_area, id, TOAST_STACK_HIT_DISMISS, toast.id.0);
                }
            }
        }
    }
}

impl NotificationQueue {
    /// Create a new notification queue with the given configuration.
    pub fn new(config: QueueConfig) -> Self {
        let dedup_window = Duration::from_millis(config.dedup_window_ms);
        Self {
            queue: VecDeque::new(),
            visible: Vec::new(),
            config,
            dedup_window,
            recent_hashes: AHashMap::new(),
            stats: QueueStats::default(),
            history: NotificationHistory::default(),
        }
    }

    /// Create a new queue with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(QueueConfig::default())
    }

    /// Push a notification to the queue.
    ///
    /// Returns `true` if the notification was accepted, `false` if it was
    /// rejected due to deduplication or queue overflow.
    pub fn push(&mut self, toast: Toast, priority: NotificationPriority) -> bool {
        self.stats.total_pushed += 1;
        let queued = QueuedNotification::new(self.apply_default_duration(toast), priority);

        // Check deduplication
        if !self.dedup_check(queued.content_hash) {
            self.stats.dedup_count += 1;
            return false;
        }

        // Check queue overflow
        if self.queue.len() >= self.config.max_queued {
            self.stats.overflow_count += 1;
            // Drop oldest low-priority item if possible
            if let Some(idx) = self.find_lowest_priority_index() {
                if self.queue[idx].priority < priority {
                    self.queue.remove(idx);
                } else {
                    return false; // New item is lower or equal priority
                }
            } else {
                return false;
            }
        }

        // Insert based on priority
        if priority == NotificationPriority::Urgent {
            // Urgent jumps to front
            self.queue.push_front(queued);
        } else {
            // Insert in priority order
            let insert_idx = self
                .queue
                .iter()
                .position(|q| q.priority < priority)
                .unwrap_or(self.queue.len());
            self.queue.insert(insert_idx, queued);
        }

        true
    }

    /// Push a notification with normal priority.
    pub fn notify(&mut self, toast: Toast) -> bool {
        self.push(toast, NotificationPriority::Normal)
    }

    /// Push an urgent notification.
    pub fn urgent(&mut self, toast: Toast) -> bool {
        self.push(toast, NotificationPriority::Urgent)
    }

    /// Dismiss a specific notification by ID.
    pub fn dismiss(&mut self, id: ToastId) {
        // Check visible toasts
        if let Some(idx) = self.visible.iter().position(|t| t.id == id)
            && !self.visible[idx].state.dismissed
        {
            self.visible[idx].dismiss();
            self.stats.user_dismissed += 1;
        }

        // Check queue — collect first to avoid double borrow
        if let Some(idx) = self.queue.iter().position(|q| q.toast.id == id) {
            if let Some(queued) = self.queue.remove(idx) {
                self.stats.user_dismissed += 1;
                self.push_to_history(&queued.toast, DismissReason::UserDismissed);
            }
        }
    }

    /// Dismiss all notifications.
    pub fn dismiss_all(&mut self) {
        let mut dismissed_visible = 0u64;
        for toast in &mut self.visible {
            if !toast.state.dismissed {
                toast.dismiss();
                dismissed_visible += 1;
            }
        }

        // Record all queued items to history before clearing.
        // Collect first to avoid double borrow on self.
        let queued_items: Vec<Toast> = self.queue.drain(..).map(|q| q.toast).collect();
        self.stats.user_dismissed += dismissed_visible + queued_items.len() as u64;
        for toast in queued_items {
            self.push_to_history(&toast, DismissReason::UserDismissed);
        }
    }

    /// Process a time tick, handling expiry and promotion.
    ///
    /// Call this regularly in your event loop (e.g., every frame or every 16ms).
    /// Returns a list of actions to perform.
    pub fn tick(&mut self, _delta: Duration) -> Vec<QueueAction> {
        let mut actions = Vec::new();

        // Clean expired dedup hashes
        let now = Instant::now();
        self.recent_hashes
            .retain(|_, t| now.saturating_duration_since(*t) < self.dedup_window);

        // Process visible toasts for expiry and animations
        let mut i = 0;
        while i < self.visible.len() {
            let toast = &mut self.visible[i];

            // Trigger auto-dismiss on expiry
            if !toast.state.dismissed && toast.is_expired() {
                toast.dismiss();
                self.stats.auto_expired += 1;
            }

            // Advance animation state
            toast.tick_animation();

            if !self.visible[i].is_visible() {
                let removed = self.visible.remove(i);
                if removed.state.dismissed && removed.config.duration.is_some() {
                    // Auto-expired if it had a duration and wasn't manually dismissed
                    self.push_to_history(&removed, DismissReason::AutoExpired);
                }
                actions.push(QueueAction::Hide(removed.id));
            } else {
                i += 1;
            }
        }

        // Promote from queue to visible: if we need to evict visible toasts
        // to make room for higher-priority items, record them in history.
        while self.visible.len() < self.config.max_visible {
            if let Some(queued) = self.queue.pop_front() {
                let id = queued.toast.id;
                self.visible.push(queued.toast);
                actions.push(QueueAction::Show(id));
            } else {
                break;
            }
        }

        actions
    }

    /// Record a toast in notification history.
    fn push_to_history(&mut self, toast: &Toast, reason: DismissReason) {

        let now = Instant::now();
        let entry = NotificationHistoryEntry {
            message: toast.content.message.clone(),
            title: toast.content.title.clone(),
            icon: toast.content.icon,
            style_variant: toast.config.style_variant,
            priority: NotificationPriority::Normal, // We don't track priority per-toast after insertion
            created_at: toast.state.created_at,
            dismissed_at: now,
            reason,
            visible_duration: now.saturating_duration_since(toast.state.created_at),
        };
        self.history.push(entry);
    }

    /// Access the notification history.
    pub fn history(&self) -> &NotificationHistory {
        &self.history
    }

    /// Clear the notification history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Get currently visible toasts.
    pub fn visible(&self) -> &[Toast] {
        &self.visible
    }

    /// Get mutable access to visible toasts.
    pub fn visible_mut(&mut self) -> &mut [Toast] {
        &mut self.visible
    }

    /// Get the number of notifications waiting in the queue.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Get the number of visible toasts.
    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    /// Get the total count (visible + pending).
    pub fn total_count(&self) -> usize {
        self.visible.len() + self.queue.len()
    }

    /// Check if the queue is empty (no visible or pending notifications).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty() && self.queue.is_empty()
    }

    /// Get queue statistics.
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }

    /// Get the configuration.
    pub fn config(&self) -> &QueueConfig {
        &self.config
    }

    /// Calculate stacking positions for all visible toasts.
    ///
    /// Returns a list of (ToastId, x, y) positions.
    pub fn calculate_positions(
        &self,
        terminal_width: u16,
        terminal_height: u16,
        margin: u16,
    ) -> Vec<(ToastId, u16, u16)> {
        let mut positions = Vec::with_capacity(self.visible.len());
        let is_top = matches!(
            self.config.position,
            ToastPosition::TopLeft | ToastPosition::TopCenter | ToastPosition::TopRight
        );

        let mut y_offset: u16 = 0;

        for toast in &self.visible {
            let (toast_width, toast_height) = toast.calculate_dimensions();
            let (base_x, base_y) = self.config.position.calculate_position(
                terminal_width,
                terminal_height,
                toast_width,
                toast_height,
                margin,
            );

            let y = if is_top {
                base_y.saturating_add(y_offset)
            } else {
                base_y.saturating_sub(y_offset)
            };

            positions.push((toast.id, base_x, y));
            y_offset = y_offset
                .saturating_add(toast_height)
                .saturating_add(self.config.stagger_offset);
        }

        positions
    }

    // --- Internal methods ---

    /// Check if a content hash is a duplicate within the dedup window.
    fn dedup_check(&mut self, hash: u64) -> bool {
        let now = Instant::now();

        // Clean old hashes
        self.recent_hashes
            .retain(|_, t| now.saturating_duration_since(*t) < self.dedup_window);

        // Check if duplicate
        if self.recent_hashes.contains_key(&hash) {
            return false;
        }

        self.recent_hashes.insert(hash, now);
        true
    }

    /// Find the index of the lowest priority item in the queue.
    fn find_lowest_priority_index(&self) -> Option<usize> {
        self.queue
            .iter()
            .enumerate()
            .min_by_key(|(_, q)| q.priority)
            .map(|(i, _)| i)
    }

    fn apply_default_duration(&self, mut toast: Toast) -> Toast {
        if !toast.config.duration_explicit {
            toast.config.duration = Some(self.config.default_duration);
            toast.config.duration_explicit = true;
        }
        toast
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// A scrollable widget that displays past notification history entries.
///
/// Renders previously dismissed, expired, and evicted toasts in a compact
/// scrollable list. Each entry shows:
///
/// - A style indicator based on the original toast's style variant
/// - The message text
/// - A dismiss reason label (dismissed, expired, evicted)
/// - How long ago the notification was active
///
/// # Example
///
/// ```ignore
/// use ftui_widgets::notification_queue::NotificationHistoryWidget;
///
/// let panel = NotificationHistoryWidget::new(&queue.history());
/// panel.render(area, frame);
/// ```
pub struct NotificationHistoryWidget<'a> {
    history: &'a NotificationHistory,
    scroll: usize,
    compact: bool,
}

impl<'a> NotificationHistoryWidget<'a> {
    /// Create a new notification history widget.
    pub fn new(history: &'a NotificationHistory) -> Self {
        Self {
            history,
            scroll: 0,
            compact: true,
        }
    }

    /// Set whether to render in compact mode (shows fewer details).
    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Set scroll offset (0 = newest first).
    #[must_use]
    pub fn scroll(mut self, offset: usize) -> Self {
        self.scroll = offset;
        self
    }
}

impl Widget for NotificationHistoryWidget<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() || self.history.is_empty() {
            return;
        }

        let deg = frame.buffer.degradation;
        if !deg.render_content() {
            return;
        }

        let max_x = area.right();
        let entries: Vec<_> = self.history.entries().collect();
        let visible_rows = area.height as usize;
        let scroll = self.scroll.min(entries.len().saturating_sub(1));
        let mut row: u16 = 0;

        for i in scroll..entries.len() {
            if row >= area.height {
                break;
            }

            let entry = &entries[i];
            let y = area.y.saturating_add(row);

            // Render the entry line
            let prefix = match entry.style_variant {
                crate::toast::ToastStyle::Success => "✓ ",
                crate::toast::ToastStyle::Error => "✗ ",
                crate::toast::ToastStyle::Warning => "! ",
                crate::toast::ToastStyle::Info => "i ",
                crate::toast::ToastStyle::Neutral => "· ",
            };

            let reason_label = match entry.reason {
                DismissReason::UserDismissed => "[dismissed]",
                DismissReason::AutoExpired => "[expired]",
                DismissReason::Evicted => "[evicted]",
            };

            let prefix_end = crate::draw_text_span(frame, area.x, y, prefix, Style::default(), max_x);

            if self.compact {
                // Compact: show icon + message + reason
                let msg_end = crate::draw_text_span(
                    frame,
                    prefix_end,
                    y,
                    &entry.message,
                    Style::default(),
                    max_x,
                );
                if visible_rows > 3 {
                    crate::draw_text_span(
                        frame,
                        msg_end,
                        y,
                        reason_label,
                        Style::default(),
                        max_x,
                    );
                }
            } else {
                // Full mode: show details
                crate::draw_text_span(
                    frame,
                    prefix_end,
                    y,
                    &entry.message,
                    Style::default(),
                    max_x,
                );
            }

            row += 1;
        }

        // Clear remaining rows if scrolled back from a longer list
        if self.compact {
            let entries_rendered = entries.len().saturating_sub(scroll).min(visible_rows);
            if entries_rendered < visible_rows {
                let empty_start = row;
                for r in empty_start..area.height {
                    let y = area.y.saturating_add(r);
                    crate::clear_text_row(frame, Rect::new(area.x, y, area.width, 1), Style::default());
                }
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
    use ftui_render::frame::Frame;
    use ftui_render::grapheme_pool::GraphemePool;
    use web_time::Duration;

    fn make_toast(msg: &str) -> Toast {
        Toast::with_id(ToastId::new(0), msg)
            .persistent()
            .no_animation() // Use persistent and no_animation for testing
    }

    fn make_ephemeral_toast(msg: &str) -> Toast {
        Toast::new(msg).no_animation()
    }

    #[test]
    fn test_queue_new() {
        let queue = NotificationQueue::with_defaults();
        assert!(queue.is_empty());
        assert_eq!(queue.visible_count(), 0);
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn test_queue_push_and_tick() {
        let mut queue = NotificationQueue::with_defaults();

        queue.push(make_toast("Hello"), NotificationPriority::Normal);
        assert_eq!(queue.pending_count(), 1);
        assert_eq!(queue.visible_count(), 0);

        // Tick promotes from queue to visible
        let actions = queue.tick(Duration::from_millis(16));
        assert_eq!(queue.pending_count(), 0);
        assert_eq!(queue.visible_count(), 1);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], QueueAction::Show(_)));
    }

    #[test]
    fn test_queue_fifo() {
        let config = QueueConfig::default().max_visible(1);
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("First"), NotificationPriority::Normal);
        queue.push(make_toast("Second"), NotificationPriority::Normal);
        queue.push(make_toast("Third"), NotificationPriority::Normal);

        queue.tick(Duration::from_millis(16));
        assert_eq!(queue.visible()[0].content.message, "First");

        // Dismiss first, tick to get second
        queue.visible_mut()[0].dismiss();
        queue.tick(Duration::from_millis(16));
        assert_eq!(queue.visible()[0].content.message, "Second");
    }

    #[test]
    fn test_queue_max_visible() {
        let config = QueueConfig::default().max_visible(2);
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.push(make_toast("B"), NotificationPriority::Normal);
        queue.push(make_toast("C"), NotificationPriority::Normal);

        queue.tick(Duration::from_millis(16));

        assert_eq!(queue.visible_count(), 2);
        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn test_queue_priority_urgent() {
        let config = QueueConfig::default().max_visible(1);
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("Normal1"), NotificationPriority::Normal);
        queue.push(make_toast("Normal2"), NotificationPriority::Normal);
        queue.push(make_toast("Urgent"), NotificationPriority::Urgent);

        queue.tick(Duration::from_millis(16));
        // Urgent should jump to front
        assert_eq!(queue.visible()[0].content.message, "Urgent");
    }

    #[test]
    fn test_queue_priority_ordering() {
        let config = QueueConfig::default().max_visible(0); // No auto-promote
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("Low"), NotificationPriority::Low);
        queue.push(make_toast("Normal"), NotificationPriority::Normal);
        queue.push(make_toast("High"), NotificationPriority::High);

        // Queue should be ordered High, Normal, Low
        let messages: Vec<_> = queue
            .queue
            .iter()
            .map(|q| q.toast.content.message.as_str())
            .collect();
        assert_eq!(messages, vec!["High", "Normal", "Low"]);
    }

    #[test]
    fn test_queue_dedup() {
        let config = QueueConfig::default().dedup_window_ms(1000);
        let mut queue = NotificationQueue::new(config);

        assert!(queue.push(make_toast("Same message"), NotificationPriority::Normal));
        assert!(!queue.push(make_toast("Same message"), NotificationPriority::Normal));

        assert_eq!(queue.stats().dedup_count, 1);
    }

    #[test]
    fn test_queue_overflow() {
        let config = QueueConfig::default().max_queued(2);
        let mut queue = NotificationQueue::new(config);

        assert!(queue.push(make_toast("A"), NotificationPriority::Normal));
        assert!(queue.push(make_toast("B"), NotificationPriority::Normal));
        // Third should fail (queue full)
        assert!(!queue.push(make_toast("C"), NotificationPriority::Normal));

        assert_eq!(queue.stats().overflow_count, 1);
    }

    #[test]
    fn test_queue_overflow_drops_lower_priority() {
        let config = QueueConfig::default().max_queued(2);
        let mut queue = NotificationQueue::new(config);

        assert!(queue.push(make_toast("Low1"), NotificationPriority::Low));
        assert!(queue.push(make_toast("Low2"), NotificationPriority::Low));
        // High priority should drop a low priority item
        assert!(queue.push(make_toast("High"), NotificationPriority::High));

        assert_eq!(queue.pending_count(), 2);
        let messages: Vec<_> = queue
            .queue
            .iter()
            .map(|q| q.toast.content.message.as_str())
            .collect();
        assert!(messages.contains(&"High"));
    }

    #[test]
    fn test_queue_dismiss() {
        let mut queue = NotificationQueue::with_defaults();

        queue.push(make_toast("Test"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        let id = queue.visible()[0].id;
        queue.dismiss(id);
        queue.tick(Duration::from_millis(16));

        assert_eq!(queue.visible_count(), 0);
        assert_eq!(queue.stats().user_dismissed, 1);
    }

    #[test]
    fn test_queue_dismiss_all() {
        let mut queue = NotificationQueue::with_defaults();

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.push(make_toast("B"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        queue.dismiss_all();
        queue.tick(Duration::from_millis(16));

        assert!(queue.is_empty());
        assert_eq!(queue.stats().user_dismissed, 2);
    }

    #[test]
    fn test_queue_calculate_positions_top() {
        let config = QueueConfig::default().position(ToastPosition::TopRight);
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.push(make_toast("B"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        let positions = queue.calculate_positions(80, 24, 1);
        assert_eq!(positions.len(), 2);

        // First toast should be at top, second below
        assert!(positions[0].2 < positions[1].2);
    }

    #[test]
    fn test_queue_calculate_positions_bottom() {
        let config = QueueConfig::default().position(ToastPosition::BottomRight);
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.push(make_toast("B"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        let positions = queue.calculate_positions(80, 24, 1);
        assert_eq!(positions.len(), 2);

        // First toast should be at bottom, second above
        assert!(positions[0].2 > positions[1].2);
    }

    #[test]
    fn test_queue_notify_helper() {
        let mut queue = NotificationQueue::with_defaults();
        assert!(queue.notify(make_toast("Normal")));
        queue.tick(Duration::from_millis(16));
        assert_eq!(queue.visible_count(), 1);
    }

    #[test]
    fn test_queue_urgent_helper() {
        let config = QueueConfig::default().max_visible(1);
        let mut queue = NotificationQueue::new(config);

        queue.notify(make_toast("Normal"));
        queue.urgent(make_toast("Urgent"));
        queue.tick(Duration::from_millis(16));

        assert_eq!(queue.visible()[0].content.message, "Urgent");
    }

    #[test]
    fn test_queue_stats() {
        let mut queue = NotificationQueue::with_defaults();

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.push(make_toast("A"), NotificationPriority::Normal); // Dedup
        queue.tick(Duration::from_millis(16));

        assert_eq!(queue.stats().total_pushed, 2);
        assert_eq!(queue.stats().dedup_count, 1);
    }

    #[test]
    fn test_queue_config_builder() {
        let config = QueueConfig::new()
            .max_visible(5)
            .max_queued(20)
            .default_duration(Duration::from_secs(10))
            .position(ToastPosition::BottomLeft)
            .stagger_offset(2)
            .dedup_window_ms(500);

        assert_eq!(config.max_visible, 5);
        assert_eq!(config.max_queued, 20);
        assert_eq!(config.default_duration, Duration::from_secs(10));
        assert_eq!(config.position, ToastPosition::BottomLeft);
        assert_eq!(config.stagger_offset, 2);
        assert_eq!(config.dedup_window_ms, 500);
    }

    #[test]
    fn test_queue_total_count() {
        let config = QueueConfig::default().max_visible(1);
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.push(make_toast("B"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        assert_eq!(queue.total_count(), 2);
        assert_eq!(queue.visible_count(), 1);
        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn queue_config_default_values() {
        let config = QueueConfig::default();
        assert_eq!(config.max_visible, 5);
        assert_eq!(config.max_queued, 10);
        assert_eq!(config.default_duration, Duration::from_secs(5));
        assert_eq!(config.position, ToastPosition::BottomRight);
        assert_eq!(config.stagger_offset, 1);
        assert_eq!(config.dedup_window_ms, 1000);
    }

    #[test]
    fn notification_priority_default_is_normal() {
        assert_eq!(
            NotificationPriority::default(),
            NotificationPriority::Normal
        );
    }

    #[test]
    fn notification_priority_ordering() {
        assert!(NotificationPriority::Low < NotificationPriority::Normal);
        assert!(NotificationPriority::Normal < NotificationPriority::High);
        assert!(NotificationPriority::High < NotificationPriority::Urgent);
    }

    #[test]
    fn queue_default_trait_delegates_to_with_defaults() {
        let queue = NotificationQueue::default();
        assert!(queue.is_empty());
        assert_eq!(queue.config().max_visible, 5);
    }

    #[test]
    fn is_empty_false_when_pending() {
        let mut queue = NotificationQueue::with_defaults();
        queue.push(make_toast("X"), NotificationPriority::Normal);
        assert!(!queue.is_empty());
    }

    #[test]
    fn is_empty_false_when_visible() {
        let mut queue = NotificationQueue::with_defaults();
        queue.push(make_toast("X"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));
        assert!(!queue.is_empty());
    }

    #[test]
    fn visible_mut_allows_modification() {
        let mut queue = NotificationQueue::with_defaults();
        queue.push(make_toast("Original"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        // Dismiss via visible_mut
        queue.visible_mut()[0].dismiss();
        queue.tick(Duration::from_millis(16));
        assert_eq!(queue.visible_count(), 0);
    }

    #[test]
    fn config_accessor_returns_config() {
        let config = QueueConfig::default().max_visible(7).stagger_offset(3);
        let queue = NotificationQueue::new(config);
        assert_eq!(queue.config().max_visible, 7);
        assert_eq!(queue.config().stagger_offset, 3);
    }

    #[test]
    fn dismiss_all_clears_queue_and_visible() {
        let config = QueueConfig::default().max_visible(1);
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.push(make_toast("B"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        // After tick: A is visible, B is pending.
        assert_eq!(queue.visible_count(), 1);
        assert_eq!(queue.pending_count(), 1);

        queue.dismiss_all();
        // dismiss_all counts both the visible and pending toast.
        assert_eq!(queue.stats().user_dismissed, 2);
        assert_eq!(queue.pending_count(), 0);

        // Next tick removes the dismissed visible toast
        queue.tick(Duration::from_millis(16));
        assert!(queue.is_empty());
    }

    #[test]
    fn dismiss_does_not_double_count_already_dismissed_visible_toast() {
        let mut queue = NotificationQueue::with_defaults();
        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        let id = queue.visible()[0].id;
        queue.dismiss(id);
        queue.dismiss(id);

        assert_eq!(queue.stats().user_dismissed, 1);
    }

    #[test]
    fn queue_applies_config_default_duration_to_default_toasts() {
        let config = QueueConfig::default().default_duration(Duration::from_secs(12));
        let mut queue = NotificationQueue::new(config);

        queue.push(make_ephemeral_toast("A"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        assert_eq!(
            queue.visible()[0].config.duration,
            Some(Duration::from_secs(12))
        );
    }

    #[test]
    fn queue_preserves_persistent_toasts_when_applying_default_duration() {
        let config = QueueConfig::default().default_duration(Duration::from_secs(12));
        let mut queue = NotificationQueue::new(config);

        queue.push(make_toast("A"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        assert_eq!(queue.visible()[0].config.duration, None);
    }

    #[test]
    fn queue_preserves_explicit_custom_duration() {
        let config = QueueConfig::default().default_duration(Duration::from_secs(12));
        let mut queue = NotificationQueue::new(config);

        queue.push(
            Toast::new("A")
                .duration(Duration::from_secs(2))
                .no_animation(),
            NotificationPriority::Normal,
        );
        queue.tick(Duration::from_millis(16));

        assert_eq!(
            queue.visible()[0].config.duration,
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn queue_preserves_explicit_duration_even_when_equal_to_toast_default() {
        let config = QueueConfig::default().default_duration(Duration::from_secs(12));
        let mut queue = NotificationQueue::new(config);

        queue.push(
            Toast::new("A")
                .duration(Duration::from_secs(5))
                .no_animation(),
            NotificationPriority::Normal,
        );
        queue.tick(Duration::from_millis(16));

        assert_eq!(
            queue.visible()[0].config.duration,
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn queue_action_equality() {
        let id = ToastId::new(42);
        assert_eq!(QueueAction::Show(id), QueueAction::Show(id));
        assert_eq!(QueueAction::Hide(id), QueueAction::Hide(id));
        assert_eq!(QueueAction::Reposition(id), QueueAction::Reposition(id));
        assert_ne!(QueueAction::Show(id), QueueAction::Hide(id));
    }

    #[test]
    fn queue_stats_default_all_zero() {
        let stats = QueueStats::default();
        assert_eq!(stats.total_pushed, 0);
        assert_eq!(stats.overflow_count, 0);
        assert_eq!(stats.dedup_count, 0);
        assert_eq!(stats.user_dismissed, 0);
        assert_eq!(stats.auto_expired, 0);
    }

    #[test]
    fn calculate_positions_empty_returns_empty() {
        let queue = NotificationQueue::with_defaults();
        let positions = queue.calculate_positions(80, 24, 1);
        assert!(positions.is_empty());
    }

    #[test]
    fn notification_stack_empty_area_renders_nothing() {
        let mut queue = NotificationQueue::with_defaults();
        queue.push(make_toast("Hello"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 10, &mut pool);
        let empty_area = Rect::new(0, 0, 0, 0);

        // Should not panic
        NotificationStack::new(&queue).render(empty_area, &mut frame);
    }

    #[test]
    fn notification_stack_margin_builder() {
        let queue = NotificationQueue::with_defaults();
        let stack = NotificationStack::new(&queue).margin(5);
        assert_eq!(stack.margin, 5);
    }

    #[test]
    fn notification_stack_renders_visible_toast() {
        let mut queue = NotificationQueue::with_defaults();
        queue.push(make_toast("Hello"), NotificationPriority::Normal);
        queue.tick(Duration::from_millis(16));

        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 10, &mut pool);
        let area = Rect::new(0, 0, 40, 10);

        NotificationStack::new(&queue)
            .margin(0)
            .render(area, &mut frame);

        let (_, x, y) = queue.calculate_positions(40, 10, 0)[0];
        let cell = frame.buffer.get(x, y).expect("cell should exist");
        assert!(!cell.is_empty(), "stack should render toast content");
    }
}
