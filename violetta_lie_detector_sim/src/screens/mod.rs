//! One module per screen of the app.

use egui::Color32;

pub mod game;
pub mod result;
pub mod start;

/// Backdrop the UI is drawn on top of (matches the page background).
pub const BACKGROUND: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x1f);

/// How a game of the lie detector ended, which the result screen reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every attempt was right.
    Success,
    /// At least one attempt was wrong.
    Failure,
    /// The verification ran out of time while the game was still going. Only a page
    /// with a session can end this way.
    Expired,
}
