//! Shared building blocks for `Corvus.exe` (launcher) and `CorvusSetup.exe` (drive builder).
//!
//! Module map:
//! - [`manifest`] — `tools.toml` schema, lenient parsing (one bad entry never sinks the file),
//!   and per-tool status checks (missing / changed / wrong environment).
//! - [`pe`]       — "are we running inside WinPE?" via the `MiniNT` registry key.
//! - [`drive`]    — locate the Corvus drive root from the exe location, else by volume label.
//! - [`theme`]    — the always-dark look, and the glow (composited shadow on live Windows,
//!   inward strokes in PE where there is no compositor).
//! - [`logging`]  — tiny file + stderr logger that never panics on a failed write.
//! - [`hash`]     — streaming SHA-256 of a file, used for optional exe integrity tags.

pub mod drive;
pub mod hash;
pub mod logging;
pub mod manifest;
pub mod pe;
pub mod theme;

pub use manifest::{Manifest, Mode, Tool, ToolStatus};
