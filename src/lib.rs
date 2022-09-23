//! Task synchronisation primitives for pinned tasks.
//!
//! This crate provides task synchronisation for `!Send` futures.

pub mod mpsc;
pub mod oneshot;
