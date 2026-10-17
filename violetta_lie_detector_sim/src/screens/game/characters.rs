//! The four characters on the stage: which of them is special, and where each
//! one stands.
//!
//! Every character is drawn from the pivot recorded in the artwork (see
//! `build.rs`), so the differently trimmed frames of an animation all hang off
//! the same spot and the animation does not shake.
//!
//! A character stands in a *slot* (left to right) and a *row* (front to back). It
//! stands in `layout::STAND_ROW` — one row upstage of the front — and in a slot of
//! its own, so the four of them are lined up across the stage and never overlap. None
//! of them is placed exactly on the centre of its slot either: each is given a small
//! random offset, so the line they stand in is placed rather than drawn. A
//! round sends each of them into a row of its own — one per depth, so no two are
//! ever the same distance from the camera — across the stage up there, and back to
//! standing depth in a new slot. That is what keeps them apart on the way: the masks
//! make them identical, so otherwise two crossing paths are impossible to tell
//! apart. One of those rows is the front of the stage itself, so a round always sends
//! one of the four across it, drawn a quarter larger than the four of them stand.
//!
//! Every leg of that walk goes along one axis: a character steps straight back,
//! crosses sideways in its row, and steps straight forward again. A leg that
//! changed slot and depth at once would read as sliding across the stage rather
//! than as walking it. One of the four does not cross anything at all — it keeps its
//! slot and only steps out of it into its row and back.
//!
//! A chaos round runs three of those passes back to back, one after the other, so
//! the four change places three times over before an answer is asked for.
//!
//! The walk cycle is shared — `npcCommon.move` — and shows the character from
//! behind, walking away, so one cycle serves every direction of travel.

use egui::{Color32, CursorIcon, Id, Sense, Stroke, StrokeKind, Ui};

use crate::assets::{self, Sprite, Textures};
use crate::screens::Mode;

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
/// couple of places on the way, so it crosses the stage sideways instead of sliding
/// straight to its new spot.
#[derive(Debug, Clone, Copy)]
pub struct Character {
    /// Whether this is the one the player has to keep track of.
    pub special: bool,
    /// How far off the centre of its slot it is placed, in window pixels, which stays
    /// the same for the whole game.
    pub offset: (f32, f32),
    /// The places it walks through this round, first to last. Outside a walk they
    /// are all the same place.
    pub path: [Spot; PLACES],
}

/// A place on the stage: which slot, and which row back from the front.
pub type Spot = (usize, usize);

/// How many places describe one pass of a character's walk, counting where it
/// starts: a pass moves it along five legs — straight back into its row, three
/// across that row, and straight forward into its new place.
pub const LEGS: usize = 6;

/// How many passes a round can shuffle for: one in a normal game, three in chaos
/// mode.
pub const MAX_PASSES: usize = 3;

/// How many places a path can hold. Each pass after the first starts in the place
/// the one before it ended, so the passes add [`LEGS`] - 1 places each.
pub const PLACES: usize = LEGS + (MAX_PASSES - 1) * (LEGS - 1);

/// How far to either side of the centre of its slot a character may be placed, and how
/// far above or below it, in window pixels.
///
/// The game does not line the four of them up exactly, so neither does this: the four
/// offsets are what keep the line of them from reading as a row of sprites. They stay
/// well inside a slot — a fifth of one at most — and smaller in height than in width,
/// because the row already tells the vertical distance and a character off its row's
/// height by much would read as floating.
const OFFSET_X: f32 = 8.0;
const OFFSET_Y: f32 = 4.0;

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
        self.path[PLACES - 1]
    }

    /// Where and how big it is drawn when it is not walking.
    pub fn stance(&self) -> Stance {
        placed(stance(self.spot()), self.offset)
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
/// The row the four stand in ([`layout::STAND_ROW`]) is the one [`CHARACTER_PIVOT_Y`]
/// is fitted for, where their feet are on the floor. A row further back is upstage,
/// i.e. into the background and up the window; the front row is one step the other
/// way, downstage and lower down the window.
pub fn anchor(spot: Spot) -> (f32, f32) {
    let distance = layout::STAND_ROW as i32 - spot.1 as i32;

    (
        layout::slot_pivot_x(spot.0),
        layout::CHARACTER_PIVOT_Y + distance as f32 * layout::DEPTH_STEP,
    )
}

/// Where and how big a character standing in `spot` is drawn.
fn stance(spot: Spot) -> Stance {
    Stance {
        anchor: anchor(spot),
        scale: scale_of(spot.1),
    }
}

/// `stance` moved by `offset`: the difference between where a slot puts a character
/// and where it has been placed in it.
fn placed(mut stance: Stance, offset: (f32, f32)) -> Stance {
    stance.anchor.0 += offset.0;
    stance.anchor.1 += offset.1;
    stance
}

/// How much bigger or smaller a character in `row` is drawn than the four of them
/// stand.
///
/// The stage is painted in perspective, so a character further back is both higher up
/// the window and smaller, which is what makes walking upstage read as leaving the
/// foreground — and the one sent across the front row is drawn larger than they stand,
/// which is what makes stepping downstage read as coming into it.
fn scale_of(row: usize) -> f32 {
    layout::DEPTH_SCALE.powi(row as i32 - layout::STAND_ROW as i32)
}

/// Spawns the four characters: one of them special, one to a slot, all of them at
/// the depth they stand at (`layout::STAND_ROW`), lined up where they can be seen.
pub fn spawn(rng: &mut Rng) -> [Character; layout::ANSWER_COUNT] {
    let mut slots: [usize; layout::ANSWER_COUNT] = std::array::from_fn(|index| index);
    rng.shuffle(&mut slots);

    let mut characters = std::array::from_fn(|index| {
        let spot = (slots[index], layout::STAND_ROW);
        Character {
            special: false,
            offset: placement(rng),
            path: [spot; PLACES],
        }
    });
    characters[rng.below(layout::ANSWER_COUNT)].special = true;
    characters
}

/// How far off the centre of its slot a character is placed: a little to one side of
/// it, and a little above or below, both drawn fresh for every game.
fn placement(rng: &mut Rng) -> (f32, f32) {
    let spread = |rng: &mut Rng, reach: f32| (rng.fraction() * 2.0 - 1.0) * reach;

    (spread(rng, OFFSET_X), spread(rng, OFFSET_Y))
}

/// Sends the characters walking to new places, through a couple of random ones on the
/// way, remembering where they started.
///
/// Each of them is given a row of its own to walk through, so the four are never at
/// the same depth while they shuffle. Every leg changes the slot or the depth and
/// never both, so the walk is a set of sideways and forward steps rather than a
/// diagonal slide.
///
/// One of the four — a different one each pass — keeps the place it is standing in
/// and walks no further than its row: it steps out of its slot into that row and back
/// into it, crossing nothing. The other three swap among the places that are left, so
/// it is the one character the player can follow without taking their eyes off the
/// rest.
///
/// A chaos round runs three passes, one after the other, so the four change places
/// three times before the player is asked to answer.
pub fn shuffle(characters: &mut [Character; layout::ANSWER_COUNT], rng: &mut Rng, mode: Mode) {
    // Where each of the four stands as the round goes on: a pass starts where the one
    // before it left them.
    let mut current: [Spot; layout::ANSWER_COUNT] =
        std::array::from_fn(|index| characters[index].spot());

    // Every character's places, collected over all the passes of the round.
    let mut paths: [[Spot; PLACES]; layout::ANSWER_COUNT] =
        std::array::from_fn(|index| [current[index]; PLACES]);

    // How many places have been written so far, which is where the next pass goes.
    let mut filled = 0;

    for pass in 0..mode.shuffle_passes() {
        let steps = shuffle_pass(&current, rng);
        // From the second pass on, its first place is where the previous one ended.
        let first_leg = if pass == 0 { 0 } else { 1 };

        for index in 0..layout::ANSWER_COUNT {
            for leg in first_leg..LEGS {
                paths[index][filled + leg - first_leg] = steps[index][leg];
            }
        }

        filled += LEGS - first_leg;
        current = std::array::from_fn(|index| steps[index][LEGS - 1]);
    }

    // A normal round fills only part of the path; the rest repeats the place it ends
    // at. `walked` is told how many legs the round is scheduled over, so the padding
    // costs no time.
    for (index, character) in characters.iter_mut().enumerate() {
        for place in &mut paths[index][filled..] {
            *place = current[index];
        }
        character.path = paths[index];
    }
}

/// One pass of a round, from the places the four stand in to the places they end the
/// pass in.
fn shuffle_pass(
    current: &[Spot; layout::ANSWER_COUNT],
    rng: &mut Rng,
) -> [[Spot; LEGS]; layout::ANSWER_COUNT] {
    // The one that keeps its place, and the three that carry on without it.
    let standing = rng.below(layout::ANSWER_COUNT);
    let moving: [usize; layout::ANSWER_COUNT - 1] =
        std::array::from_fn(|index| if index < standing { index } else { index + 1 });

    let mut slots: [usize; layout::ANSWER_COUNT - 1] =
        std::array::from_fn(|index| current[moving[index]].0);
    rng.shuffle(&mut slots);

    // Every one of the three has to end up somewhere else, or a pass would have two
    // characters that stayed where they were. A shuffle that leaves any of them its
    // own place is replaced by one of the two ways of cycling all three.
    let swapped = (0..slots.len()).all(|index| slots[index] != current[moving[index]].0);
    if !swapped {
        let cycle = [[1, 2, 0], [2, 0, 1]][rng.below(2)];
        slots = cycle.map(|index| current[moving[index]].0);
    }

    // Where each character ends up, the one that keeps its place among them.
    let mut destinations: [usize; layout::ANSWER_COUNT] =
        std::array::from_fn(|index| current[index].0);
    for (index, character) in moving.iter().enumerate() {
        destinations[*character] = slots[index];
    }

    let mut rows = layout::WALK_ROWS;
    rng.shuffle(&mut rows);

    std::array::from_fn(|index| {
        let start = current[index];
        let destination = (destinations[index], layout::STAND_ROW);
        // Into its own row for the walk — one of the four crosses the front of the
        // stage there, a quarter larger than they stand, while the others step upstage
        // of themselves — across that row twice behind the others, and back to standing
        // depth in the new slot. The waypoints avoid both the slot it starts in and the
        // one it is headed for, so every sideways leg really does cross the stage.
        let row = rows[index];
        let away = (start.0, row);

        if index == standing {
            // Nothing to cross: it steps out of its slot into its row and back into it,
            // waiting there while the others cross. The places it is already at are
            // repeated because every path has the same shape, and that wait is what
            // keeps it at its own depth while the other three are in theirs.
            [start, away, away, away, away, destination]
        } else {
            let first = waypoint(row, &[start.0, destination.0], rng);
            let second = waypoint(row, &[first.0, destination.0], rng);
            [
                start,
                away,
                first,
                second,
                (destination.0, row),
                destination,
            ]
        }
    })
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
    let slot = (0..layout::ANSWER_COUNT)
        .find(|slot| !avoid.contains(slot))
        .unwrap_or(0);
    (slot, row)
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
    /// How many legs the walk is laid out over. Every character is given the same
    /// number, so the four change depth at the same moments and never share a depth
    /// while they are moving.
    pub legs: usize,
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
            walked(&characters[index], step.walked, step.legs)
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
/// path, so it heads along the axes the path is built from during one round, and its
/// size follows the rows it walks through on the way.
///
/// Every character is walked over the same number of legs, including the ones that go
/// nowhere (the character that keeps its place waits in its row while the others cross).
/// That shared schedule is what keeps their depths apart: with the same leg at the same
/// time, the four step into their rows together, sit at four different depths while
/// they cross, and step back together. Counting only the legs that move would let one
/// of them still be stepping into its row while another is already parked at that
/// depth, and the two would be drawn over one another.
fn walked(character: &Character, progress: f32, legs: usize) -> Stance {
    if legs == 0 {
        return character.stance();
    }

    let travelled = (progress * legs as f32).clamp(0.0, legs as f32);
    let leg = (travelled as usize).min(legs - 1);
    let part = travelled - leg as f32;

    let from = stance(character.path[leg]);
    let to = stance(character.path[leg + 1]);
    placed(
        Stance {
            anchor: (
                from.anchor.0 + (to.anchor.0 - from.anchor.0) * part,
                from.anchor.1 + (to.anchor.1 - from.anchor.1) * part,
            ),
            scale: from.scale + (to.scale - from.scale) * part,
        },
        character.offset,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::game::rng;

    /// A character is walking, not sliding: every leg changes the slot it is in or
    /// the depth it is at, and never both at once. A leg that did would be drawn as
    /// a diagonal slide across the stage.
    #[test]
    fn a_round_walks_along_one_axis_at_a_time() {
        let mut rng = Rng::new(rng::seed(0.0));
        let mut characters = spawn(&mut rng);

        for round in 0..64 {
            shuffle(&mut characters, &mut rng, Mode::Normal);

            for character in characters {
                let mut steps = 0;
                for leg in character.path.windows(2) {
                    let (from, to) = (leg[0], leg[1]);
                    assert!(
                        from.0 == to.0 || from.1 == to.1,
                        "round {round}: {from:?} to {to:?} moves obliquely"
                    );
                    steps += usize::from(from != to);
                }
                // Even the one that keeps its place takes a step out and a step back; a
                // path of nothing but legs that go nowhere would leave a character
                // standing there with the walk cycle running.
                assert!(
                    steps >= 2,
                    "round {round}: {character:?} takes {steps} steps"
                );
            }
        }
    }

    /// Chaos mode runs three passes of the shuffle back to back, so each character
    /// changes places three times over — and every pass still takes the four out of
    /// their slots and back along one axis at a time.
    #[test]
    fn chaos_mode_shuffles_three_times_over() {
        // The path array is sized for the most passes a round can run.
        assert_eq!(Mode::Chaos.shuffle_passes(), MAX_PASSES);

        let mut rng = Rng::new(rng::seed(5.0));
        let mut characters = spawn(&mut rng);
        shuffle(&mut characters, &mut rng, Mode::Chaos);

        let mut total = 0;
        for character in characters {
            assert_eq!(character.spot().1, layout::STAND_ROW);

            let mut steps = 0;
            for leg in character.path.windows(2) {
                assert!(leg[0].0 == leg[1].0 || leg[0].1 == leg[1].1);
                steps += usize::from(leg[0] != leg[1]);
            }
            // Five legs a pass for the three that move, two for the one that keeps
            // its place: a character that kept it every pass takes six.
            assert!(matches!(steps, 6..=15), "a chaos round takes {steps} steps");
            total += steps;
        }
        // Three passes of three movers and one stayer.
        assert_eq!(total, 3 * (3 * 5 + 2));
    }

    /// One character a round keeps the place it is standing in: it walks no further
    /// than its row, so it crosses no slots, while the other three swap among the
    /// places that are left. Which of the four it is changes from round to round.
    #[test]
    fn one_character_a_round_keeps_its_own_place() {
        let mut rng = Rng::new(rng::seed(3.0));
        let mut characters = spawn(&mut rng);
        let mut kept = Vec::new();

        for _ in 0..64 {
            let before: Vec<usize> = characters
                .iter()
                .map(|character| character.spot().0)
                .collect();
            shuffle(&mut characters, &mut rng, Mode::Normal);

            // The four still stand one to a slot…
            let mut after: Vec<usize> = characters
                .iter()
                .map(|character| character.spot().0)
                .collect();
            after.sort_unstable();
            after.dedup();
            assert_eq!(after.len(), layout::ANSWER_COUNT);

            // …and exactly one of them in the slot it was already standing in.
            let staying: Vec<usize> = (0..layout::ANSWER_COUNT)
                .filter(|index| characters[*index].spot().0 == before[*index])
                .collect();
            assert_eq!(
                staying.len(),
                1,
                "characters in their old slots: {staying:?}"
            );

            // That one never leaves its slot, and every other one does.
            for (index, character) in characters.iter().enumerate() {
                let crosses = character.path.windows(2).any(|span| span[0].0 != span[1].0);
                assert_eq!(crosses, index != staying[0], "character {index}");
            }

            kept.push(staying[0]);
        }

        // It is not always the same one of the four.
        kept.sort_unstable();
        kept.dedup();
        assert!(
            kept.len() > 1,
            "the same character kept its place every round"
        );
    }

    /// Every character keeps a depth of its own while they shuffle, which is what
    /// tells the four of them apart when their paths cross — and none of them walks
    /// at the depth the four of them stand at, so no step into the walk or back out
    /// of it is a character pausing in place.
    #[test]
    fn a_round_keeps_the_four_at_four_depths_of_its_own() {
        let mut rng = Rng::new(rng::seed(1.0));
        let mut characters = spawn(&mut rng);

        for _ in 0..64 {
            shuffle(&mut characters, &mut rng, Mode::Normal);

            let mut rows: Vec<usize> = characters
                .iter()
                .map(|character| character.path[1].1)
                .collect();
            rows.sort_unstable();
            rows.dedup();
            assert_eq!(rows.len(), layout::ANSWER_COUNT);
            assert!(!rows.contains(&layout::STAND_ROW));
        }
    }

    /// One character of the four walks across the front of the stage every round,
    /// where it is drawn at full size rather than the smaller size the four of them
    /// stand at.
    #[test]
    fn a_round_sends_one_character_across_the_front_row() {
        let mut rng = Rng::new(rng::seed(2.0));
        let mut characters = spawn(&mut rng);

        for _ in 0..64 {
            shuffle(&mut characters, &mut rng, Mode::Normal);

            let front = characters
                .iter()
                .filter(|character| character.path[1].1 == layout::FRONT_ROW)
                .count();
            assert_eq!(front, 1);

            // And all four stand at the same depth between rounds, one row upstage.
            for character in characters {
                assert_eq!(character.spot().1, layout::STAND_ROW);
            }
        }
    }

    /// The four never share a depth while they walk. With one shared leg schedule they
    /// step into their rows together, sit at four different depths while they cross, and
    /// step back together — two characters at the same depth would be drawn over one
    /// another, which is exactly what a shared schedule avoids.
    #[test]
    fn the_four_never_share_a_depth_while_they_walk() {
        let mut rng = Rng::new(rng::seed(11.0));
        let mut characters = spawn(&mut rng);

        for mode in [Mode::Normal, Mode::Chaos] {
            let legs = mode.shuffle_passes() * (LEGS - 1);
            for round in 0..8 {
                shuffle(&mut characters, &mut rng, mode);

                // Fine enough to catch a crossing at the moment it happens: an old
                // schedule where two characters met at a depth could do so inside one
                // frame's worth of progress.
                for step in 1..1000 {
                    let progress = step as f32 / 1000.0;
                    for left in 0..layout::ANSWER_COUNT {
                        for right in (left + 1)..layout::ANSWER_COUNT {
                            let a = walked(&characters[left], progress, legs);
                            let b = walked(&characters[right], progress, legs);
                            // `walked` includes each character's own placement offset;
                            // take it back off so two depths really compare.
                            let a_depth = (a.anchor.1 - characters[left].offset.1, a.scale);
                            let b_depth = (b.anchor.1 - characters[right].offset.1, b.scale);
                            assert!(
                                (a_depth.0 - b_depth.0).abs() > 0.01
                                    || (a_depth.1 - b_depth.1).abs() > 0.0001,
                                "{mode:?} round {round}, progress {progress:.2}: \
                                 characters {left} and {right} share a depth"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The four are placed a little apart rather than lined up: each one sits off the
    /// centre of its slot, and never far enough off that two of them read as one.
    #[test]
    fn the_four_are_placed_a_little_apart() {
        for seed in 0..8 {
            let mut rng = Rng::new(rng::seed(seed as f64));
            let characters = spawn(&mut rng);

            for character in characters {
                assert!(character.offset.0.abs() <= OFFSET_X);
                assert!(character.offset.1.abs() <= OFFSET_Y);
            }

            // The closest two of them can come, once the offsets have moved them, is a
            // slot apart less the two offsets.
            for left in 0..layout::ANSWER_COUNT {
                for right in (left + 1)..layout::ANSWER_COUNT {
                    let apart = (characters[left].stance().anchor.0
                        - characters[right].stance().anchor.0)
                        .abs();
                    assert!(
                        apart > layout::SLOT_STEP - 2.0 * OFFSET_X,
                        "seed {seed}: {left} and {right} are {apart} px apart"
                    );
                }
            }
        }
    }

    /// The depth is measured from the row the four stand at, so that is where they are
    /// drawn at their own size with their feet on the floor; the front row is a quarter
    /// larger and lower down the window, and the rows they walk deeper are smaller and
    /// higher up.
    #[test]
    fn the_four_are_drawn_at_their_own_size() {
        assert_eq!(scale_of(layout::STAND_ROW), 1.0);
        assert_eq!(anchor((0, layout::STAND_ROW)).1, layout::CHARACTER_PIVOT_Y);

        let front = scale_of(layout::FRONT_ROW);
        assert!(
            (front - 1.25).abs() < 0.001,
            "the front row is drawn at {front}"
        );
        assert!(anchor((0, layout::FRONT_ROW)).1 > layout::CHARACTER_PIVOT_Y);

        for row in layout::WALK_ROWS {
            if row > layout::STAND_ROW {
                assert!(
                    scale_of(row) < 1.0,
                    "row {row} is drawn at {}",
                    scale_of(row)
                );
                assert!(anchor((0, row)).1 < layout::CHARACTER_PIVOT_Y);
            }
        }
    }
}
