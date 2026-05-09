//! frostx - project lifecycle management library.
//!
//! A folder becomes a managed project by placing a `frostx.toml` inside it.
//! This library provides everything needed to initialize projects, scan for
//! inactivity, evaluate rules, and execute action pipelines.
//!
//! The `frostx` binary is a thin CLI adapter over [`ops`]. Applications that
//! want to embed frostx logic can call [`ops`] functions directly and handle
//! output however they choose.

/// The [`Action`](actions::Action) trait and all built-in action implementations.
pub mod actions;
/// Backup backends (rsync, ssh).
pub mod backup;
/// CLI type definitions for the `frostx` binary and build utilities.
pub mod cli;
/// Configuration: `frostx.toml` parsing, include resolution, UUID management.
pub mod config;
/// Rich error formatting: TOML snippets and action name suggestions.
pub mod diagnostics;
/// Error types and exit code constants.
pub mod error;
/// High-level operations: init, check, run, scan, doctor, gc, projects.
pub mod ops;
/// Output data structures and renderers (human-readable and JSON/NDJSON).
pub mod output;
/// Pipeline: rule evaluation and action execution engine.
pub mod pipeline;
/// Filesystem scanner: last-modified timestamp detection.
pub mod scanner;
