//! The faint figures the stage lamps throw up.
//!
//! Once the masks are on, a scatter of ghostly figures — many faint copies of the
//! same standing artwork the characters are drawn from — is laid over the stage in
//! random places and at random strengths. It is texture, not characters: the
//! scatter is a mass of half-seen shapes, so that one of the four moving behind it
//! is that much harder to follow.
//!
//! It changes with the round, because a scatter that stayed put would be something
//! the eye could learn, and it goes out with the lamps: the caller hands it the
//! fraction of the lamps' strength that is showing, and it fades with them.

use egui::{Color32, Ui};

use crate::assets::{self, Textures};

use super::layout;
use super::rng::Rng;
use super::stage::{Stage, draw_scaled_tinted};

/// How many figures are scattered.
const FIGURES: usize = 24;
/// The strengths a figure can be drawn at, drawn at random for each of them. They
/// are faint on purpose: a stronger figure would read as a character on the stage
/// rather than as a shape in front of it.
const ALPHAS: [u8; 4] = [0, 20, 40, 60];
/// How far inside the opening a figure's centre is kept, so that enough of every
/// figure is inside it to be worth seeing — the frame masks the rest.
const MARGIN: f32 = 24.0;

/// One figure of the scatter: where its centre goes, and what strength it is drawn
/// at when the lamps are at full.
#[derive(Debug, Clone, Copy)]
struct Figure {
    centre: (f32, f32),
    alpha: u8,
}

/// A scatter of figures, drawn afresh for every round.
#[derive(Debug, Clone, Copy)]
pub struct Pattern {
    figures: [Figure; FIGURES],
}

impl Pattern {
    /// Scatters a fresh set over the stage opening.
    pub fn scatter(rng: &mut Rng) -> Self {
        let (left, top, width, height) = layout::OPENING;
        let spot = |rng: &mut Rng, from: f32, extent: f32| {
            from + MARGIN + rng.fraction() * (extent - MARGIN * 2.0).max(0.0)
        };

        Self {
            figures: std::array::from_fn(|_| Figure {
                centre: (spot(rng, left, width), spot(rng, top, height)),
                alpha: ALPHAS[rng.below(ALPHAS.len())],
            }),
        }
    }

    /// Draws the scatter at `fade` of its own strength, which is what takes it out
    /// with the lamps rather than leaving it on the darkened stage.
    pub fn draw(&self, ui: &Ui, textures: &mut Textures, stage: &Stage, fade: f32) {
        let Some(figure) = textures.get(ui.ctx(), assets::PATTERN_FIGURE) else {
            return;
        };

        for ghost in &self.figures {
            // A figure drawn at nothing is left out rather than drawn invisibly,
            // which is a quarter of them.
            let alpha = (ghost.alpha as f32 * fade).round() as u8;
            if alpha == 0 {
                continue;
            }

            draw_scaled_tinted(
                ui,
                stage,
                &figure,
                ghost.centre,
                1.0,
                Color32::from_white_alpha(alpha),
            );
        }
    }
}
