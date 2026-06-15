//! Cooperative cancellation for agent turns.
//!
//! A [`CancelSignal`] is a cheap, cloneable flag the run loop polls at turn
//! boundaries. Setting it (e.g. from a Ctrl-C handler) asks the current turn to
//! stop after the in-flight step, so the partial session is preserved rather than
//! lost to a hard process kill.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A shared, latching cancellation flag. Clones observe the same state, so a
/// handler thread can cancel a turn running elsewhere. Default is "not
/// cancelled", so a harness built without one simply never stops early.
#[derive(Clone, Debug, Default)]
pub struct CancelSignal {
    flag: Arc<AtomicBool>,
}

impl CancelSignal {
    /// A fresh, un-cancelled signal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Latching: once set it stays set.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::CancelSignal;

    #[test]
    fn starts_uncancelled_and_latches() {
        let signal = CancelSignal::new();
        assert!(!signal.is_cancelled());
        signal.cancel();
        assert!(signal.is_cancelled());
        // Latching: still cancelled on a re-check.
        assert!(signal.is_cancelled());
    }

    #[test]
    fn clones_share_state() {
        let signal = CancelSignal::new();
        let clone = signal.clone();
        signal.cancel();
        assert!(clone.is_cancelled());
    }
}
