//! Task synchronisation primitives for pinned tasks.
//!
//! This crate provides task synchronisation for `!Send` futures.

#![deny(
    missing_docs,
    missing_debug_implementations,
    bare_trait_objects,
    anonymous_parameters,
    elided_lifetimes_in_paths
)]

mod barrier;
mod cell;
pub mod mpsc;
pub mod oneshot;

pub use barrier::*;
