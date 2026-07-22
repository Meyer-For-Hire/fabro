//! Durable storage for server-managed MCP server definitions.
//!
//! Concrete SQLite-backed [`McpServerStore`] with a synchronous catalog cache,
//! SHA-256 revisions for optimistic concurrency, and one-time import from the
//! legacy per-file TOML directory. The domain model lives in `fabro-types`;
//! this crate owns persistence.

mod error;
mod model;
mod store;

pub use error::McpServerStoreError;
pub use fabro_db::ImportReport;
pub use store::{McpServerStore, import_legacy_directory_once};
