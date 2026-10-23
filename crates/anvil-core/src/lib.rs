//! Shared launcher services. The caller owns the Tokio runtime and operation lifetime.
pub mod account;
pub mod account_store;
mod catalog_cache;
pub mod client;
pub mod client_plan;
pub mod context;
pub mod curseforge;
pub mod error;
mod extract;
pub mod fabric;
pub mod java;
pub mod launch;
pub mod metadata;
mod platform;
pub mod resources;
pub mod server;
mod storage;
mod temurin;

pub use context::{Context, Paths};
pub use error::{Error, Result};
pub use tokio_util::sync::CancellationToken;
