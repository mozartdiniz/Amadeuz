//! iCloud Notes access over CloudKit Web Services.
//!
//! This crate has no GTK dependency so it can be built and tested on any platform
//! (including macOS, where the GTK client itself does not build). See
//! `ICLOUD_NOTES_PROTOCOL.md` at the repository root for the protocol research.

pub mod auth;
pub mod cloudkit;
pub mod crdt;
pub mod newnote;
pub mod notebody;
pub mod pbdump;
pub mod table;
mod srp;
pub mod wire;

pub use auth::{ICloudClient, SignInOutcome, TrustedPhone};
pub use cloudkit::{ICloudFolder, ICloudNote, Snapshot};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("incorrect Apple ID or password")]
    BadCredentials,
    #[error("incorrect verification code")]
    BadCode,
    #[error("not signed in to iCloud")]
    NotSignedIn,
    #[error("iCloud session expired")]
    SessionExpired,
    #[error("Apple requires attention on this account: {0}")]
    AccountActionRequired(String),
    #[error("the note changed on another device; reload it and try again")]
    Conflict,
    #[error("unexpected iCloud response: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;
