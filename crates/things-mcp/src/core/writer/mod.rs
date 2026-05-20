//! Write path: JSON URL construction, executor seam, post-write SQLite poll.
//!
//! Sibling of `core/reader/`. See `docs/superpowers/specs/2026-05-20-plan-4-writer-infra-design.md`.

pub mod executor;
pub mod operation;
pub mod outcome;
pub mod secret;
pub mod url;
pub mod verify;
pub mod writer;
