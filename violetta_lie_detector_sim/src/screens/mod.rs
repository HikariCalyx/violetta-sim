//! One module per screen of the app.

use egui::Color32;

pub mod game;
pub mod result;
pub mod start;

/// Backdrop the UI is drawn on top of (matches the page background).
pub const BACKGROUND: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x1f);

/// The red a chaos-mode button is filled with, so it reads as the harder of the two
/// settings rather than as another way in.
pub const CHAOS_FILL: Color32 = Color32::from_rgb(0x84, 0x00, 0x00);

/// How far a chaos-mode button sits below the button above it.
pub const CHAOS_GAP: f32 = 16.0;

/// How a game of the lie detector ended, which the result screen reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every attempt was right.
    Success,
    /// At least one attempt was wrong.
    Failure,
}

/// Which of the game's two settings a game is played at.
///
/// Chaos mode is the second button on the title screen: the four are shuffled for
/// longer and three times as far, the lamps throw up twice as many figures at half
/// again their strength, and the player has less time to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The game as it is normally played.
    Normal,
    /// The same game, made harder to follow.
    Chaos,
}

impl Mode {
    /// How long the characters take to change places.
    ///
    /// The game's own UI data names this as its shuffle animation delay, at 2500 ms;
    /// the walk here is given longer, because every leg of it goes along one axis
    /// rather than straight to the next place.
    pub fn walk_seconds(self) -> f64 {
        match self {
            Self::Normal => 5.0,
            Self::Chaos => 7.0,
        }
    }

    /// How many times the four change places in one round.
    ///
    /// Each pass carries every character out of its slot into a row of its own and
    /// back to standing depth in a new one, so chaos mode shuffles the stage three
    /// times over before it asks for an answer.
    pub fn shuffle_passes(self) -> usize {
        match self {
            Self::Normal => 1,
            Self::Chaos => 3,
        }
    }

    /// How long the player has to pick a position.
    pub fn answer_seconds(self) -> f64 {
        match self {
            Self::Normal => 5.0,
            Self::Chaos => 3.0,
        }
    }
}
