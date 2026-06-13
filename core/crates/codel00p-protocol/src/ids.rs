//! Stable string newtypes for sessions, turns, and events.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
static TURN_COUNTER: AtomicU64 = AtomicU64::new(1);
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(1);

macro_rules! id_type {
    ($name:ident, $prefix:literal, $counter:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                let id = $counter.fetch_add(1, Ordering::Relaxed);
                Self(format!("{}-{id}", $prefix))
            }

            pub fn from_static(value: &'static str) -> Self {
                Self(value.to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_type!(SessionId, "session", SESSION_COUNTER);
id_type!(TurnId, "turn", TURN_COUNTER);
id_type!(EventId, "event", EVENT_COUNTER);
