//! codel00p cloud service: the team control-plane HTTP API.
//!
//! The service is the source of truth for product data (projects, and later
//! provider policy, shared memory, and audit), reusing the engine crates and
//! persisting through `codel00p-storage`. Identity and membership come from
//! Clerk: every request carries a Clerk session JWT, verified at the boundary
//! and projected into an [`AuthContext`].

pub mod agents;
pub mod auth;
pub mod error;
pub mod mcp;
pub mod memory;
pub mod projects;
mod routes;
mod state;

pub use auth::{AuthContext, JwtVerifier, VerifierError, clerk_frontend_api_from_publishable_key};
pub use error::ApiError;
pub use routes::app;
pub use state::{AppState, ChangeEvent, storage_from_env};
