//! UI building blocks and helper functions.
//!
//! This module provides reusable UI helpers for buttons, panels, badges, and other
//! common UI patterns. These are simple functions that return GPUI elements, promoting
//! DRY principles without the overhead of full component abstractions.

pub mod badges;
pub mod buttons;
pub mod panels;

// Re-export for convenience
pub use badges::*;
pub use buttons::*;
pub use panels::*;
