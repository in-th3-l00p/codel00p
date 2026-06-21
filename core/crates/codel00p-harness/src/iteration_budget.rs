use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct IterationBudget {
    max_total: u32,
    used: AtomicU32,
}

impl IterationBudget {
    pub fn new(max_total: u32) -> Self {
        Self {
            max_total,
            used: AtomicU32::new(0),
        }
    }

    pub fn consume(&self) -> bool {
        let mut current = self.used.load(Ordering::Relaxed);
        loop {
            if current >= self.max_total {
                return false;
            }

            match self.used.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
    }

    pub fn refund(&self) {
        let mut current = self.used.load(Ordering::Relaxed);
        loop {
            if current == 0 {
                return;
            }

            match self.used.compare_exchange_weak(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(next) => current = next,
            }
        }
    }

    pub fn used(&self) -> u32 {
        self.used.load(Ordering::SeqCst)
    }

    pub fn remaining(&self) -> u32 {
        self.max_total.saturating_sub(self.used())
    }

    /// The maximum number of iterations this budget permits.
    pub fn max_total(&self) -> u32 {
        self.max_total
    }
}
