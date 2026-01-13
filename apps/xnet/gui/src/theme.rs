//! Color palette and spacing constants for the xnet GUI.
//!
//! This module provides a cohesive dark theme based on Catppuccin Mocha.
//! Colors are organized by category: backgrounds, text, accents, and buttons.

use gpui::{Hsla, Pixels, px, rgb};

// ---------------------------------------------------------------------------
// Background Colors
// ---------------------------------------------------------------------------

/// Primary background color for the window.
pub fn bg_primary() -> Hsla {
    rgb(0x1E1E2E).into()
}

/// Surface background color for panels and cards.
pub fn bg_surface() -> Hsla {
    rgb(0x2A2A3C).into()
}

/// Surface hover state.
#[allow(dead_code)]
pub fn bg_surface_hover() -> Hsla {
    rgb(0x353548).into()
}

// ---------------------------------------------------------------------------
// Text Colors
// ---------------------------------------------------------------------------

/// Primary text color (high contrast).
pub fn text_primary() -> Hsla {
    rgb(0xCDD6F4).into()
}

/// Secondary text color (medium contrast).
pub fn text_secondary() -> Hsla {
    rgb(0xA6ADC8).into()
}

/// Muted text color (low contrast, for disabled states).
pub fn text_muted() -> Hsla {
    rgb(0x6C7086).into()
}

// ---------------------------------------------------------------------------
// Accent Colors
// ---------------------------------------------------------------------------

/// Green accent (success, running states).
pub fn accent_green() -> Hsla {
    rgb(0xA6E3A1).into()
}

/// Red accent (error, danger states).
pub fn accent_red() -> Hsla {
    rgb(0xF38BA8).into()
}

/// Blue accent (info, primary actions).
pub fn accent_blue() -> Hsla {
    rgb(0x89B4FA).into()
}

/// Yellow accent (warning, pending states).
pub fn accent_yellow() -> Hsla {
    rgb(0xF9E2AF).into()
}

/// Mauve accent (special highlights).
pub fn accent_mauve() -> Hsla {
    rgb(0xCBA6F7).into()
}

// ---------------------------------------------------------------------------
// Button Colors
// ---------------------------------------------------------------------------

/// Primary button background.
pub fn btn_primary() -> Hsla {
    rgb(0x89B4FA).into()
}

/// Primary button hover state.
#[allow(dead_code)]
pub fn btn_primary_hover() -> Hsla {
    rgb(0xB4D0FB).into()
}

/// Danger button background.
pub fn btn_danger() -> Hsla {
    rgb(0xF38BA8).into()
}

/// Danger button hover state.
#[allow(dead_code)]
pub fn btn_danger_hover() -> Hsla {
    rgb(0xF5A0B8).into()
}

/// Success button background.
pub fn btn_success() -> Hsla {
    rgb(0xA6E3A1).into()
}

/// Success button hover state.
#[allow(dead_code)]
pub fn btn_success_hover() -> Hsla {
    rgb(0xBDEBB9).into()
}

/// Warning button background.
pub fn btn_warning() -> Hsla {
    rgb(0xF9E2AF).into()
}

/// Warning button hover state.
#[allow(dead_code)]
pub fn btn_warning_hover() -> Hsla {
    rgb(0xFBECC8).into()
}

/// Button text color (dark, for use on colored buttons).
pub fn btn_text() -> Hsla {
    rgb(0x1E1E2E).into()
}

// ---------------------------------------------------------------------------
// Spacing Constants
// ---------------------------------------------------------------------------

/// Extra small spacing (4px).
#[allow(dead_code)]
pub fn spacing_xs() -> Pixels {
    px(4.0)
}

/// Small spacing (8px).
#[allow(dead_code)]
pub fn spacing_sm() -> Pixels {
    px(8.0)
}

/// Medium spacing (12px).
#[allow(dead_code)]
pub fn spacing_md() -> Pixels {
    px(12.0)
}

/// Large spacing (16px).
#[allow(dead_code)]
pub fn spacing_lg() -> Pixels {
    px(16.0)
}

/// Extra large spacing (20px).
#[allow(dead_code)]
pub fn spacing_xl() -> Pixels {
    px(20.0)
}

// ---------------------------------------------------------------------------
// Border Radius Constants
// ---------------------------------------------------------------------------

/// Small border radius (4px).
#[allow(dead_code)]
pub fn radius_sm() -> Pixels {
    px(4.0)
}

/// Medium border radius (6px).
#[allow(dead_code)]
pub fn radius_md() -> Pixels {
    px(6.0)
}

/// Large border radius (8px).
#[allow(dead_code)]
pub fn radius_lg() -> Pixels {
    px(8.0)
}
