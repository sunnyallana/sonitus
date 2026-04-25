//! Play queue with shuffle, repeat, and history.
//!
//! The queue is a flat `Vec<TrackId>` plus a cursor and metadata flags.
//! Shuffle is implemented by maintaining a parallel "shuffled order" that
//! gets regenerated whenever shuffle is toggled or a track is appended.

use crate::player::commands::RepeatMode;
use rand::seq::SliceRandom;

/// Identifier type used by the queue. Always a Sonitus track UUID string.
pub type TrackId = String;

/// Mutable, in-memory play queue.
#[derive(Debug, Clone, Default)]
pub struct PlayQueue {
    /// User-facing order of tracks.
    items: Vec<TrackId>,
    /// Index into `items` for the currently-playing item.
    cursor: Option<usize>,
    /// Stack of recently-played track IDs, for `Prev`.
    history: Vec<TrackId>,
    /// Shuffle state.
    shuffle: bool,
    /// Shuffled-order indices into `items`. `shuffled[shuffle_cursor]` is
    /// the next track to play when shuffle is on.
    shuffled: Vec<usize>,
    /// Cursor into `shuffled` (when shuffle is on).
    shuffle_cursor: Option<usize>,
    /// Repeat mode.
    repeat: RepeatMode,
}

impl PlayQueue {
    /// Construct an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of items in the queue.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True if no items are queued.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Snapshot of the queue in playback order. Shuffle does not reorder
    /// the displayed list — only the next-track logic.
    pub fn snapshot(&self) -> &[TrackId] {
        &self.items
    }

    /// Currently-playing track, if any.
    pub fn current(&self) -> Option<&TrackId> {
        self.cursor.and_then(|i| self.items.get(i))
    }

    /// Repeat mode.
    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    /// Whether shuffle is enabled.
    pub fn is_shuffle(&self) -> bool {
        self.shuffle
    }

    /// Replace the entire queue. Cursor jumps to first item.
    pub fn replace(&mut self, items: Vec<TrackId>) {
        self.items = items;
        self.cursor = if self.items.is_empty() { None } else { Some(0) };
        self.history.clear();
        self.regenerate_shuffle();
    }

    /// Append to the end of the queue.
    pub fn enqueue(&mut self, id: TrackId) {
        self.items.push(id);
        if self.cursor.is_none() {
            self.cursor = Some(self.items.len() - 1);
        }
        self.regenerate_shuffle();
    }

    /// Insert immediately after the current track.
    pub fn enqueue_next(&mut self, id: TrackId) {
        let idx = self.cursor.map(|c| c + 1).unwrap_or(0);
        self.items.insert(idx, id);
        self.regenerate_shuffle();
    }

    /// Clear everything except the currently-playing track.
    pub fn clear(&mut self) {
        if let Some(c) = self.cursor {
            let current = self.items.remove(c);
            self.items = vec![current];
            self.cursor = Some(0);
        } else {
            self.items.clear();
        }
        self.history.clear();
        self.regenerate_shuffle();
    }

    /// Set repeat mode.
    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    /// Toggle shuffle.
    pub fn set_shuffle(&mut self, on: bool) {
        if self.shuffle == on { return; }
        self.shuffle = on;
        self.regenerate_shuffle();
    }

    /// Advance to the next track per repeat + shuffle. Returns the new
    /// current track, or `None` if the queue is exhausted.
    pub fn next(&mut self) -> Option<&TrackId> {
        if let Some(cur) = self.cursor.and_then(|i| self.items.get(i).cloned()) {
            self.history.push(cur);
        }

        if matches!(self.repeat, RepeatMode::One) {
            // RepeatOne: stay on the same item.
            return self.current();
        }

        if self.shuffle {
            // Use shuffled cursor.
            let next = self.shuffle_cursor.map(|c| c + 1).unwrap_or(0);
            if next >= self.shuffled.len() {
                if matches!(self.repeat, RepeatMode::All) {
                    self.regenerate_shuffle_only();
                    self.shuffle_cursor = self.shuffled.first().map(|_| 0);
                    self.cursor = self.shuffled.first().copied();
                } else {
                    self.shuffle_cursor = None;
                    self.cursor = None;
                }
            } else {
                self.shuffle_cursor = Some(next);
                self.cursor = self.shuffled.get(next).copied();
            }
        } else {
            let next = self.cursor.map(|c| c + 1).unwrap_or(0);
            if next >= self.items.len() {
                if matches!(self.repeat, RepeatMode::All) && !self.items.is_empty() {
                    self.cursor = Some(0);
                } else {
                    self.cursor = None;
                }
            } else {
                self.cursor = Some(next);
            }
        }
        self.current()
    }

    /// Go back to the previous track. If we don't have history, restart
    /// the current track.
    pub fn prev(&mut self) -> Option<&TrackId> {
        if let Some(prev) = self.history.pop() {
            // Find that ID in items and set cursor.
            if let Some((i, _)) = self.items.iter().enumerate().find(|(_, id)| *id == &prev) {
                self.cursor = Some(i);
                if self.shuffle {
                    if let Some((si, _)) = self.shuffled.iter().enumerate().find(|(_, idx)| **idx == i) {
                        self.shuffle_cursor = Some(si);
                    }
                }
                return self.items.get(i);
            }
        }
        self.current()
    }

    fn regenerate_shuffle(&mut self) {
        if !self.shuffle {
            self.shuffled.clear();
            self.shuffle_cursor = None;
            return;
        }
        self.regenerate_shuffle_only();
        // Place cursor on what's currently playing.
        if let Some(cur) = self.cursor {
            if let Some((sc, _)) = self.shuffled.iter().enumerate().find(|(_, i)| **i == cur) {
                self.shuffle_cursor = Some(sc);
            }
        }
    }

    fn regenerate_shuffle_only(&mut self) {
        let mut indices: Vec<usize> = (0..self.items.len()).collect();
        let mut rng = rand::rng();
        indices.shuffle(&mut rng);
        self.shuffled = indices;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<TrackId> {
        (0..n).map(|i| format!("track-{i}")).collect()
    }

    #[test]
    fn next_progresses_through_queue() {
        let mut q = PlayQueue::new();
        q.replace(ids(3));
        assert_eq!(q.current(), Some(&"track-0".into()));
        q.next();
        assert_eq!(q.current(), Some(&"track-1".into()));
        q.next();
        assert_eq!(q.current(), Some(&"track-2".into()));
        q.next();
        assert!(q.current().is_none());
    }

    #[test]
    fn repeat_all_loops_to_first() {
        let mut q = PlayQueue::new();
        q.replace(ids(3));
        q.set_repeat(RepeatMode::All);
        q.next();
        q.next();
        q.next();
        assert_eq!(q.current(), Some(&"track-0".into()));
    }

    #[test]
    fn repeat_one_stays_on_same_item() {
        let mut q = PlayQueue::new();
        q.replace(ids(3));
        q.set_repeat(RepeatMode::One);
        q.next();
        q.next();
        assert_eq!(q.current(), Some(&"track-0".into()));
    }

    #[test]
    fn prev_uses_history() {
        let mut q = PlayQueue::new();
        q.replace(ids(3));
        q.next();
        q.next(); // cursor at track-2
        q.prev();
        assert_eq!(q.current(), Some(&"track-1".into()));
    }

    #[test]
    fn enqueue_next_inserts_after_current() {
        let mut q = PlayQueue::new();
        q.replace(ids(3));
        q.enqueue_next("inserted".into());
        let snap = q.snapshot().to_vec();
        assert_eq!(snap, vec![
            "track-0".to_string(),
            "inserted".to_string(),
            "track-1".to_string(),
            "track-2".to_string(),
        ]);
    }

    #[test]
    fn clear_keeps_only_currently_playing() {
        let mut q = PlayQueue::new();
        q.replace(ids(3));
        q.next(); // cursor on track-1
        q.clear();
        assert_eq!(q.snapshot(), &["track-1".to_string()]);
        assert_eq!(q.current(), Some(&"track-1".into()));
    }

    #[test]
    fn shuffle_regenerates_with_new_state() {
        let mut q = PlayQueue::new();
        q.replace(ids(50));
        q.set_shuffle(true);
        // Just check that toggling shuffle doesn't crash and the cursor
        // remains valid.
        for _ in 0..5 {
            q.next();
        }
        assert!(q.current().is_some());
    }
}
