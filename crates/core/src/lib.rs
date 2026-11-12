//! Reads the local data of the coding agents agent-dashboard covers, Claude Code, Copilot CLI and
//! Copilot in VS Code, and keeps the usage history behind it.
//!
//! The words used here are defined in `CONTEXT.md` at the repository root.

pub mod agent;
pub mod copilot;
pub mod history;
pub mod http;
pub mod import;
pub mod ingest;
mod lenient;
pub mod limits;
pub mod price_refresh;
pub mod pricing;
pub mod reconcile;
pub mod report;
pub mod roots;
pub mod stats_cache;
pub mod tokens;
pub mod transcript;
pub mod usage_endpoint;
pub mod vscode;
