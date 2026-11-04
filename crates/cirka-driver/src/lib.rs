//! cirka-driver: the shared pipeline from common Core to an executable,
//! used by both `achc` and `cirka`.

pub mod cli;
pub mod config;
pub mod native;
pub mod pipeline;

pub use config::Config;
