//! Milling definitions, deterministic toolpaths and independent NC verification.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod collision;
pub mod compare;
pub mod corpus;
mod dimensions;
pub mod generate;
mod model;
pub mod nc;
mod spatial;
mod stock_mesh;
mod surface;
pub mod verify;
pub use model::*;

#[derive(Debug, thiserror::Error)]
#[error("CAM: {0}")]
pub struct Error(pub String);
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(Error(message.into()))
}

pub const VERSION: &str = "cam/5";
pub const TOLERANCE: f64 = 0.01;
pub const POST_ERROR: f64 = 0.001;
pub const VERIFY_ERROR: f64 = 0.0025;
pub const MAX_MOVES: usize = 200_000;
pub const MAX_ITEMS: usize = 1000;

pub fn fingerprint<T: serde::Serialize>(value: &T) -> Result<String> {
    use sha2::{Digest, Sha256};
    let data = serde_json::to_vec(&(VERSION, value)).map_err(|e| Error(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(data)))
}
