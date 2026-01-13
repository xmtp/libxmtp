//! UI building blocks and helper functions.
//!
//! This module provides reusable UI helpers for panels, badges, and other
//! common UI patterns. These are simple functions that return GPUI elements, promoting
//! DRY principles without the overhead of full component abstractions.
//!
//! Buttons use `gpui_component::Button` directly — see `views/root.rs`.

pub mod badges;
pub mod panels;

// Re-export for convenience
pub use badges::*;
pub use panels::*;
