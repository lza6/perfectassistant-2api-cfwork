//! PerfectAssistant2API 本地网关库

pub mod api;
pub mod config;
pub mod errors;
pub mod models;
pub mod protocol;
pub mod upstream;
pub mod web;

pub use api::{build_router, AppState};
pub use config::Config;
pub use errors::ApiError;
pub use models::ModelRegistry;
pub use upstream::UpstreamClient;
