//! The four characters on the stage: which of them is special, and where each
//! one stands.
//!
//! Every character is drawn from the pivot recorded in the artwork (see
//! `build.rs`), so the differently trimmed frames of an animation all hang off
//! the same spot and the animation does not shake.
//!
//! A character stands in a *slot* (left to right) and a *row* (front to back). It
//! stands in the front row and in a slot of its own, so the four of them are
//! lined up across the stage and never overlap. A round sends each of them back
//! into a row of its own — one per depth, so no two are ever the same distance
//! from the camera — across the stage up there, and forward again into a new slot.
//! That is what keeps them apart on the way: the masks make them identical, so
//! otherwise two crossing paths are impossible to tell apart.
//!
//! The walk cycle is shared — `npcCommon.move` — and shows the character from
//! behind, walking away, so one cycle serves every direction of travel.

use egui::{Color32, CursorIcon, Id, Sense, Stroke, StrokeKind, Ui};

use crate::assets::{self, Sprite, Textures};

use super::layout;
use super::rng::Rng;
use super::stage::{Stage, draw_pivoted_scaled};

/// The artwork a character is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    /// Standing in the open, which is when the special character gives itself
    /// away.
    Stand,
    /// Putting the mask on; the last frame of it is the masked stand.
    MaskOn,
    /// Standing masked, waiting to be picked.
    Masked,
    /// Walking to another position.
    Walk,
    /// Taking the mask off, which reveals the special character.
    MaskOff,
}

/// One of the characters on the stage.
///
/// Where it stands is the last place on its path: every round sends it through a
/// couple of places on the way, so it crosses the stage in a few directions
/// instead of sliding straight to its new spot.
#[derive(Debug, Clone, Copy)]
pub struct Character {
    /// Whether this is the one the player has to keep track of.
    pub special: bool,
    /// The places it walks through this round, first to last. Outside a walk they
    /// are all the same place.
    pub path: [Spot; LEGS],
}

/// A place on the stage: which slot, and which row back from the front.
pub type Spot = (usize, usize);

/// How many places describe a character's walk, counting where it starts: a round
/// moves it along three legs, changing direction twice.
pub const LEGS: usize = 4;

/// The rectangle drawn around the character the pointer is over, so it is obvious
/// which one a click would choose.
///
/// The game itself only relies on its cursor and on the buttons' hover artwork;
/// the characters have no highlighted state, so this is drawn instead.
const HOVER_STROKE: Color32 = Color32::from_rgb(0xa8, 0xdc, 0xff);
/// Its thickness, in window pixels.
const HOVER_WIDTH: f32 = 2.0;

impl Character {
    /// The place the character stands in when it is not walking.
    pub fn spot(&self) -> Spot {
        self.path[LEGS - 1]
    }

    /// Where and how big it is drawn when it is not walking.
    pub fn stance(&self) -> Stance {
        stance(self.spot())
    }
}

/// Where a character is drawn: the point its pivot goes on, and how much smaller
/// it is for standing further back.
#[derive(Debug, Clone, Copy)]
pub struct Stance {
    pub anchor: (f32, f32),
    pub scale: f32,
}

/// Where a character standing in `spot` has its pivot.
///
/// Row 0 is the front of the stage, where the fitted `CHARACTER_PIVOT_Y` puts the
/// character's feet on the floor; each row further back is upstage, i.e. into the
/// background.
pub fn anchor(spot: Spot) -> (f32, f32) {
    (
        layout::slot_pivot_x(spot.0),
        layout::CHARACTER_PIVOT_Y - spot.1 as f32 * layout::DEPTH_STEP,
    )
}

/// Where and how big a character standing in `spot` is drawn.
fn stance(spot: Spot) -> Stance {
    Stance {
        anchor: anchor(spot),
        scale: scale_of(spot.1),
    }
}

/// How much smaller a character in `row` is drawn. The stage is painted in
/// perspective, so a character further back is both higher up the window and
/// smaller, which is what makes walking upstage read as leaving the foreground.
fn scale_of(row: usize) -> f32 {
    layout::DEPTH_SCALE.powi(row as i32)
}

/// Spawns the four characters: one of them special, one to a slot, all of them in
/// the front row where they can be seen.
pub fn spawn(rng: &mut Rng) -> [Character; layout::ANSWER_COUNT] {
    let mut slots: [usize; layout::ANSWER_COUNT] = std::array::from_fn(|index| index);
    rng.shuffle(&mut slots);

    let mut characters = std::array::from_fn(|index| {
        let spot = (slots[index], layout::FRONT_ROW);
        Character {
            special: false,
            path: [spot; LEGS],
        }
    });
    characters[rng.below(layout::ANSWER_COUNT)].special = true;
    characters
}

/// Sends every character walking to a new place, through a couple of random ones
/// on the way, remembering where it started.
///
/// Each of them is given a row of its own to walk through, so the four are never
/// at the same depth while they shuffle. A character that is handed its own place
/// back is sent walking anyway, out to its waypoints and back: it ends up where it
/// started, but it does not stand still in plain sight of the player.
pub fn shuffle(characters: &mut [Character; layout::ANSWER_COUNT], rng: &mut Rng) {
    let mut slots: [usize; layout::ANSWER_COUNT] =
        std::array::from_fn(|index| characters[index].spot().0);
    rng.shuffle(&mut slots);

    // A shuffle can hand every character its own place back. Nothing would move
    // then, so shift the slots along by one instead.
    if (0..layout::ANSWER_COUNT).all(|index| slots[index] == characters[index].spot().0) {
        slots.rotate_left(1);
    }

    let mut rows = layout::WALK_ROWS;
    rng.shuffle(&mut rows);

    for (index, character) in characters.iter_mut().enumerate() {
        let start = character.spot();
        let destination = (slots[index], layout::FRONT_ROW);
        // Step back into its own row — shrinking as it goes — wander across up
        // there behind the others, and come forward into the new place at full
        // size.
        let row = rows[index];
        let first = waypoint(row, &[start.0, destination.0], rng);
        let second = waypoint(row, &[first.0], rng);
        character.path = [start, first, second, destination];
    }
}

/// A place for a character to walk through: its own row, in a slot outside
/// `avoid`, so that the leg that reaches it really does cross the stage.
fn waypoint(row: usize, avoid: &[usize], rng: &mut Rng) -> Spot {
    for _ in 0..8 {
        let slot = rng.below(layout::ANSWER_COUNT);
        if !avoid.contains(&slot) {
            return (slot, row);
        }
    }
    // The stage is small, so a couple of tries always find a spare slot; this is
    // only a guard against looping forever if it ever does not.
    ((avoid[0] + 1) % layout::ANSWER_COUNT, row)
}

/// The slot the special character is standing in.
pub fn special_slot(characters: &[Character; layout::ANSWER_COUNT]) -> usize {
    characters
        .iter()
        .find(|character| character.special)
        .map_or(0, |character| character.spot().0)
}

/// Where and how big the special character is drawn, which is what the glow rings.
pub fn special_stance(characters: &[Character; layout::ANSWER_COUNT]) -> Stance {
    characters
        .iter()
        .find(|character| character.special)
        .map_or(
            Stance {
                anchor: (0.0, 0.0),
                scale: 1.0,
            },
            Character::stance,
        )
}

/// How the four characters are drawn in one frame.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    /// The artwork they are drawn with.
    pub pose: Pose,
    /// The frame counter of that artwork.
    pub tick: usize,
    /// How far a walk to another place has come, from 0 to 1. Only [`Pose::Walk`]
    /// uses it.
    pub walked: f32,
    /// Whether the characters can be clicked to answer.
    pub pickable: bool,
}

/// Draws all four characters, and reports the slot of the one the player clicked.
///
/// The characters are targets themselves while `step.pickable` is set, so pointing
/// at one counts as choosing it, not just pressing the button below it.
pub fn draw(
    ui: &Ui,
    textures: &mut Textures,
    stage: &Stage,
    characters: &[Character; layout::ANSWER_COUNT],
    step: Step,
) -> Option<usize> {
    let mut clicked = None;

    let stances: [Stance; layout::ANSWER_COUNT] = std::array::from_fn(|index| {
        if step.pose == Pose::Walk {
            walked(&characters[index], step.walked)
        } else {
            characters[index].stance()
        }
    });

    // The furthest back first, so a character walking in front of another covers
    // it rather than the other way round.
    let mut order: [usize; layout::ANSWER_COUNT] = std::array::from_fn(|index| index);
    order.sort_by(|left, right| stances[*left].scale.total_cmp(&stances[*right].scale));

    for index in order {
        let character = &characters[index];
        let stance = stances[index];

        let Some(sprite) = artwork(ui, textures, character, step.pose, step.tick + index * 2)
        else {
            continue;
        };
        let rect = draw_pivoted_scaled(ui, stage, &sprite, stance.anchor, stance.scale);

        // Only a live character takes clicks, so pointing at one in another phase
        // cannot answer for the player.
        let sense = if step.pickable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, Id::new(("character", index)), sense);
        if step.pickable && response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            ui.painter().rect_stroke(
                rect,
                0.0,
                Stroke::new(HOVER_WIDTH * stage.scale(), HOVER_STROKE),
                StrokeKind::Outside,
            );
        }
        if response.clicked() {
            clicked = Some(character.spot().0);
        }
    }

    clicked
}

/// Where and how big a character is while walking: `progress` runs along the whole
/// path, so it heads in a few different directions during one round, and its size
/// follows the rows it walks through on the way.
fn walked(character: &Character, progress: f32) -> Stance {
    let legs = LEGS - 1;
    let travelled = (progress * legs as f32).clamp(0.0, legs as f32);
    let leg = (travelled as usize).min(legs - 1);
    let part = travelled - leg as f32;

    let from = stance(character.path[leg]);
    let to = stance(character.path[leg + 1]);
    Stance {
        anchor: (
            from.anchor.0 + (to.anchor.0 - from.anchor.0) * part,
            from.anchor.1 + (to.anchor.1 - from.anchor.1) * part,
        ),
        scale: from.scale + (to.scale - from.scale) * part,
    }
}

/// The frame `character` is drawn with.
fn artwork(
    ui: &Ui,
    textures: &mut Textures,
    character: &Character,
    pose: Pose,
    tick: usize,
) -> Option<Sprite> {
    let [idle, mask_on, mask_off] = artwork_groups(character.special);

    match pose {
        Pose::Stand => textures.frame(ui.ctx(), idle, tick),
        Pose::MaskOn => textures.frame_once(ui.ctx(), mask_on, tick),
        // Standing masked is where the mask animation settles.
        Pose::Masked => textures.last_frame(ui.ctx(), mask_on),
        Pose::MaskOff => textures.frame_once(ui.ctx(), mask_off, tick),
        // Every character that is moving plays the walk cycle, which loops because
        // a swap lasts longer than one cycle of it. Drawing one of them from the
        // stand instead would leave it sliding across the stage as a still picture.
        Pose::Walk => textures.frame(ui.ctx(), assets::CHARACTER_WALK, tick),
    }
}

/// The standing, masking and unmasking artwork of a character.
fn artwork_groups(special: bool) -> [&'static str; 3] {
    if special {
        [
            assets::SPECIAL_CHARACTER_IDLE,
            assets::SPECIAL_CHARACTER_MASK_ON,
            assets::SPECIAL_CHARACTER_MASK_OFF,
        ]
    } else {
        [
            assets::CHARACTER_IDLE,
            assets::CHARACTER_MASK_ON,
            assets::CHARACTER_MASK_OFF,
        ]
    }
}
