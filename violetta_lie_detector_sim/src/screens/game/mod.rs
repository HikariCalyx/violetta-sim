//! The lie detector window and the round it plays.
//!
//! The composition follows the artwork. `lieDetector.back1.png` is the window
//! frame and is drawn *last*, so the visible part of the stage is exactly its
//! 412x187 black opening:
//!
//! 1. the stage backdrop, at the top-left of the opening;
//! 2. the stage lights, at 60% once the masks are on, cycling their colour twice a
//!    second;
//! 3. the animated layer — the four characters, and the glow on the special one —
//!    drawn after the backdrop and before the frame, so the frame masks whatever
//!    moves out of the opening;
//! 4. the window frame;
//! 5. the instruction line, the device readout and the answer buttons;
//! 6. the `READY`, `START`, success and fail banners, which play over it all.
//!
//! A game runs through [`Phase`]: a countdown, the characters appearing while the
//! glow marks the special one, `READY`, `START`, the masks going on as the lights
//! come up, then rounds of changing places and picking a position — four of them,
//! or fewer when one runs out of time, which ends the game — and finally the masks
//! coming off and a banner for the outcome.

mod characters;
mod layout;
mod rng;
mod stage;
mod widgets;

use egui::{
    Align2, Area, CentralPanel, Color32, Context, CornerRadius, Frame, Id, Margin, Rect, RichText,
    Ui,
};

use crate::assets::{self, Textures};
use crate::net::{Plan, Verification};
use crate::screens::{BACKGROUND, Outcome};

use characters::{Character, Pose};
use rng::Rng;
use stage::{
    Stage, WINDOW_ORIGIN, draw_pivoted, draw_pivoted_scaled, draw_pivoted_tinted, draw_scaled,
};

/// How many positions the stage has, i.e. answers and rounds per game.
const ANSWERS: usize = layout::ANSWER_COUNT;

/// Frames per second of the animations that loop.
const IDLE_FPS: f64 = 6.0;
const GLOW_FPS: f64 = 12.0;
const LIGHT_FPS: f64 = 12.0;
/// Frames per second of the animations that play once.
const READY_FPS: f64 = 10.0;
const MASK_FPS: f64 = 10.0;
const BANNER_FPS: f64 = 8.0;
const WALK_FPS: f64 = 8.0;

/// The countdown shows one digit per this many seconds, from five down to one.
const COUNTDOWN_SECONDS: f64 = 5.0;
/// How many digits the countdown counts through.
const COUNTDOWN_DIGITS: usize = 5;
/// How long the glow rests on the special character before the round opens.
const REVEAL_SECONDS: f64 = 5.0;
/// How long the characters take to change places.
///
/// The game's own UI data names this as its shuffle animation delay (2500 ms),
/// and the walk is long enough for the characters to make their way into the
/// background and back.
const WALK_SECONDS: f64 = 2.5;
/// How long the player has to pick a position.
const ANSWER_SECONDS: f64 = 5.0;
/// How much of the stage lights shows once the masks are on, and how long they
/// take to come up or go out again.
const LIGHT_ALPHA: f32 = 0.60;
const LIGHT_FADE_SECONDS: f64 = 0.5;
/// How often the stage lights change colour, and the colours they cycle through:
/// the lamp of a roulette wheel, stepping on twice a second.
///
/// The tint *multiplies* the artwork, so a phase is a hue with one channel left at
/// full rather than a saturated colour — that keeps the artwork's own shading, and
/// the light, while the other two channels take the colour of the hue.
const LIGHT_PHASE_SECONDS: f64 = 0.5;
const LIGHT_PHASES: [Color32; 6] = [
    Color32::from_rgb(0xff, 0xff, 0xff),
    Color32::from_rgb(0xff, 0x60, 0x60),
    Color32::from_rgb(0xff, 0xc8, 0x50),
    Color32::from_rgb(0x60, 0xff, 0x60),
    Color32::from_rgb(0x60, 0x90, 0xff),
    Color32::from_rgb(0xc0, 0x70, 0xff),
];

/// The colour the device's timer gauge fills with, matching the artwork's clock.
const TIMER_FILL: Color32 = Color32::from_rgb(0xd9, 0x22, 0x1a);

/// What the screen is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// The big digits that run before the window opens.
    Countdown,
    /// The characters are on the stage and the glow marks the special one.
    Reveal,
    /// The `READY` stamp plays.
    Ready,
    /// The `START` banner plays.
    Start,
    /// The masks go on while the stage lights come up.
    MaskOn,
    /// The characters walk to new positions.
    Swap,
    /// The player picks a position.
    Pick,
    /// The lights go out and the masks come off.
    MaskOff,
    /// The banner for a game in which every attempt was right.
    Success,
    /// The banner for a game in which an attempt was wrong.
    Failed,
}

/// The lie detector screen.
pub struct GameScreen {
    phase: Phase,
    /// Value of `InputState::time` at which the current phase started.
    phase_started: f64,
    /// Randomness for the character positions and the swaps.
    rng: Rng,
    /// The four characters, in the order they were spawned.
    characters: [Character; ANSWERS],
    /// The position the player picked in each round; `None` for the rounds that
    /// were never reached, because an earlier one ran out of time.
    picks: [Option<u8>; ANSWERS],
    /// Whether each round's pick was the special character.
    verdicts: [bool; ANSWERS],
    /// Which round is being played.
    round: usize,
    /// The shuffle the Worker handed out, when this page is a verification: one
    /// destination slot per character, per round. Without it every round shuffles
    /// itself, which is what the plain simulator does.
    orders: Option<Vec<Vec<usize>>>,
    /// Set once the last banner has played, which ends the game.
    done: Option<Outcome>,
}

impl GameScreen {
    pub fn new() -> Self {
        let mut rng = Rng::new(rng::seed(0.0));
        let characters = characters::spawn(&mut rng);

        Self {
            phase: Phase::Countdown,
            phase_started: 0.0,
            rng,
            characters,
            picks: [None; ANSWERS],
            verdicts: [false; ANSWERS],
            round: 0,
            orders: None,
            done: None,
        }
    }

    /// Starts a new game: a fresh set of characters, and the artwork decoded
    /// while the countdown is on screen.
    pub fn restart(&mut self, ctx: &Context, textures: &mut Textures, time: f64) {
        self.orders = None;
        self.begin(ctx, textures, time);
    }

    /// Starts a new game of a verification, following the Worker's shuffle.
    ///
    /// The characters are still laid out by the local generator — where they stand
    /// before the first swap is not scored, and drawing it fresh keeps the reveal
    /// different every time — but which of them is special, and where each one ends
    /// up, come from the Worker, because those are what it scores a pick against.
    pub fn restart_verified(
        &mut self,
        ctx: &Context,
        textures: &mut Textures,
        time: f64,
        plan: &Plan,
    ) {
        self.begin(ctx, textures, time);
        characters::set_special(&mut self.characters, plan.special);
        self.orders = Some(plan.orders.clone());
    }

    /// The part of a new game that both entry points share.
    fn begin(&mut self, ctx: &Context, textures: &mut Textures, time: f64) {
        self.rng = Rng::new(rng::seed(time));
        self.characters = characters::spawn(&mut self.rng);
        self.picks = [None; ANSWERS];
        self.verdicts = [false; ANSWERS];
        self.round = 0;
        self.done = None;
        self.phase = Phase::Countdown;
        self.phase_started = time;
        preload(ctx, textures);
    }

    /// Draws the screen and handles input, returning the outcome once the game
    /// has played its last banner.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        textures: &mut Textures,
        verification: Option<&Verification>,
    ) -> Option<Outcome> {
        let time = ui.input(|input| input.time);

        // The clock is the Worker's and it does not pause for the game: once it runs
        // out the member has already been removed, so the page stops where it stands.
        // A game that has already made up its mind is left alone, so that the last
        // banner — the game's own feedback — is never cut off mid-animation.
        let finishing = self.done.is_some()
            || self.round >= ANSWERS
            || matches!(self.phase, Phase::Success | Phase::Failed);
        if !finishing {
            if let Some(verification) = verification {
                let expired = verification.remaining(time) <= 0.0
                    || verification
                        .snapshot()
                        .is_some_and(|snapshot| snapshot.status == "expired");
                if expired {
                    return Some(Outcome::Expired);
                }
            }
        }

        self.advance(time, verification);
        // Every phase animates, so keep the frames coming.
        ui.ctx().request_repaint();

        CentralPanel::default()
            .frame(Frame::NONE.fill(BACKGROUND))
            .show(ui, |ui| {
                let stage = Stage::new(ui.max_rect());
                if !stage.is_visible() {
                    return;
                }

                // The window has not opened yet, so the countdown runs on its own.
                if self.phase == Phase::Countdown {
                    self.draw_countdown(ui, textures, &stage, time);
                    return;
                }

                self.draw_backdrop(ui, textures, &stage);
                self.draw_lights(ui, textures, &stage, time);
                let clicked = self.draw_characters(ui, textures, &stage, time);
                self.draw_ready_stamp(ui, textures, &stage, time);
                self.draw_window(ui, textures, &stage);
                if let Some(message) = self.banner_message() {
                    draw_banner(ui, textures, &stage, message);
                }
                self.draw_readout(ui, textures, &stage);
                self.draw_timer(ui, textures, &stage, time);
                self.draw_buttons(ui, textures, &stage, time, clicked, verification);
                self.draw_banner_animation(ui, textures, &stage, time);
            });

        if let Some(verification) = verification {
            draw_verification(ui, verification, time);
        }

        self.done
    }

    /// Moves the round along, phase by phase.
    fn advance(&mut self, time: f64, verification: Option<&Verification>) {
        let elapsed = time - self.phase_started;

        match self.phase {
            Phase::Countdown => {
                if elapsed >= COUNTDOWN_SECONDS {
                    self.enter(Phase::Reveal, time);
                }
            }
            Phase::Reveal => {
                if elapsed >= REVEAL_SECONDS {
                    self.enter(Phase::Ready, time);
                }
            }
            Phase::Ready => {
                if elapsed >= seconds_of(assets::READY_STAMP, READY_FPS) {
                    self.enter(Phase::Start, time);
                }
            }
            Phase::Start => {
                if elapsed >= seconds_of(assets::START_BANNER, BANNER_FPS) {
                    self.enter(Phase::MaskOn, time);
                }
            }
            Phase::MaskOn => {
                if elapsed >= seconds_of(assets::CHARACTER_MASK_ON, MASK_FPS) {
                    self.enter(Phase::Swap, time);
                }
            }
            Phase::Swap => {
                if elapsed >= WALK_SECONDS {
                    self.enter(Phase::Pick, time);
                }
            }
            Phase::Pick => {
                if elapsed >= ANSWER_SECONDS {
                    // Running out of time ends the game where it stands: the rounds
                    // that were left are never played. The Worker counts it as a failed
                    // attempt (slot zero is "no answer"), because the attempt it holds
                    // open would otherwise never be closed.
                    if let Some(verification) = verification {
                        verification.report_pick(self.round, 0, elapsed * 1000.0);
                    }
                    self.enter(Phase::MaskOff, time);
                }
            }
            Phase::MaskOff => {
                if elapsed >= LIGHT_FADE_SECONDS + seconds_of(assets::CHARACTER_MASK_OFF, MASK_FPS)
                {
                    // An attempt that was wrong, a round that ran out of time and
                    // every round that was never played all leave `false` here, so
                    // only four right answers are a success.
                    let phase = if self.verdicts.iter().all(|found| *found) {
                        Phase::Success
                    } else {
                        Phase::Failed
                    };
                    self.enter(phase, time);
                }
            }
            Phase::Success => {
                if elapsed >= seconds_of(assets::SUCCESS_BANNER, BANNER_FPS) {
                    self.done = Some(Outcome::Success);
                }
            }
            Phase::Failed => {
                if elapsed >= seconds_of(assets::FAIL_BANNER, BANNER_FPS) {
                    self.done = Some(Outcome::Failure);
                }
            }
        }
    }

    /// Switches to `phase`, setting up whatever that phase starts from.
    fn enter(&mut self, phase: Phase, time: f64) {
        self.phase = phase;
        self.phase_started = time;
        if phase == Phase::Swap {
            match &self.orders {
                // A verification plays the shuffle the Worker scored the pick against.
                Some(orders) => {
                    let round = self.round.min(orders.len().saturating_sub(1));
                    characters::shuffle_to(&mut self.characters, &mut self.rng, &orders[round]);
                }
                None => characters::shuffle(&mut self.characters, &mut self.rng),
            }
        }
    }

    /// Records the position the player picked for the current round and moves on
    /// to the next one.
    fn pick(&mut self, position: usize, time: f64, verification: Option<&Verification>) {
        let special = characters::special_slot(&self.characters);
        self.verdicts[self.round] = position == special;
        self.picks[self.round] = Some(position as u8 + 1);

        // The Worker scores this, and its answer is the one that decides whether the
        // member is allowed to talk in the group again.
        if let Some(verification) = verification {
            let elapsed = time - self.phase_started;
            verification.report_pick(self.round, position as u8 + 1, elapsed * 1000.0);
        }

        self.round += 1;
        if self.round == ANSWERS {
            self.enter(Phase::MaskOff, time);
        } else {
            self.enter(Phase::Swap, time);
        }
    }

    /// The instruction line for the current phase.
    ///
    /// The messages are the game's own artwork, in the order the game numbers
    /// them: the first while the characters are on the stage, the second while a
    /// position is being picked, and the third as the answer is given.
    fn banner_message(&self) -> Option<&'static str> {
        match self.phase {
            Phase::Countdown => None,
            Phase::Reveal | Phase::Ready | Phase::Start => Some(assets::MESSAGE_WATCH),
            Phase::MaskOn | Phase::Swap | Phase::Pick => Some(assets::MESSAGE_PICK),
            Phase::MaskOff | Phase::Success | Phase::Failed => Some(assets::MESSAGE_ANSWER),
        }
    }

    /// The artwork the characters are drawn with, and its frame counter.
    fn pose(&self, time: f64, elapsed: f64) -> (Pose, usize) {
        match self.phase {
            Phase::Countdown => (Pose::Stand, 0),
            Phase::Reveal | Phase::Ready | Phase::Start => {
                (Pose::Stand, (time * IDLE_FPS) as usize)
            }
            Phase::MaskOn => (Pose::MaskOn, (elapsed * MASK_FPS) as usize),
            Phase::Swap => (Pose::Walk, (elapsed * WALK_FPS) as usize),
            Phase::Pick => (Pose::Masked, 0),
            // The masks only come off once the lights have finished going out.
            Phase::MaskOff => (
                Pose::MaskOff,
                (((elapsed - LIGHT_FADE_SECONDS).max(0.0)) * MASK_FPS) as usize,
            ),
            // The masks stay off once they are off, so the reveal is held.
            Phase::Success | Phase::Failed => (Pose::MaskOff, (elapsed * MASK_FPS) as usize),
        }
    }

    /// How strongly the stage lights are showing right now.
    fn light_alpha(&self, elapsed: f64) -> f32 {
        let ramp = |from: f32, to: f32| {
            let progress = (elapsed / LIGHT_FADE_SECONDS).clamp(0.0, 1.0) as f32;
            from + (to - from) * progress
        };

        match self.phase {
            Phase::MaskOn => ramp(0.0, LIGHT_ALPHA),
            Phase::Swap | Phase::Pick => LIGHT_ALPHA,
            Phase::MaskOff => ramp(LIGHT_ALPHA, 0.0),
            _ => 0.0,
        }
    }

    /// The countdown that runs before the window opens.
    ///
    /// It is shown on the game's standby dialog, with the seconds left written into
    /// the blank the dialog leaves in its instruction line.
    fn draw_countdown(&self, ui: &Ui, textures: &mut Textures, stage: &Stage, time: f64) {
        let elapsed = time - self.phase_started;
        let step = ((elapsed / (COUNTDOWN_SECONDS / COUNTDOWN_DIGITS as f64)) as usize)
            .min(COUNTDOWN_DIGITS - 1);

        let Some(dialog) = textures.get(ui.ctx(), assets::STANDBY_DIALOG) else {
            return;
        };
        // The dialog is centred on the screen, which is the point the detector
        // window itself takes when it opens. Its own pivot is its top-left corner.
        let size = dialog.size();
        let origin = (
            layout::SCREEN_CENTRE.0 - size.x / 2.0,
            layout::SCREEN_CENTRE.1 - size.y / 2.0,
        );
        draw_pivoted(ui, stage, &dialog, origin);

        // The dialog's frames are the digits themselves: frame 5 is a `5`.
        let Some(name) = assets::frame_of(assets::STANDBY_DIGIT, COUNTDOWN_DIGITS - step) else {
            return;
        };
        let Some(digit) = textures.get(ui.ctx(), name) else {
            return;
        };
        let (x, y) = layout::STANDBY_COUNT;
        draw_pivoted(ui, stage, &digit, (origin.0 + x, origin.1 + y));
    }

    /// The backdrop of the stage, filling the window opening.
    fn draw_backdrop(&self, ui: &Ui, textures: &mut Textures, stage: &Stage) {
        if let Some(backdrop) = textures.get(ui.ctx(), assets::STAGE_BACKDROP) {
            draw_pivoted(ui, stage, &backdrop, WINDOW_ORIGIN);
        }
    }

    /// The stage lights, which come up while the masks go on and go out again
    /// before the masks come off.
    ///
    /// Which colour they take is read from the clock rather than from the phase,
    /// so the cycle keeps ticking across the rounds instead of restarting with
    /// every shuffle.
    fn draw_lights(&self, ui: &Ui, textures: &mut Textures, stage: &Stage, time: f64) {
        let alpha = self.light_alpha(time - self.phase_started);
        if alpha <= 0.0 {
            return;
        }

        let tick = (time * LIGHT_FPS) as usize;
        if let Some(lights) = textures.frame(ui.ctx(), assets::LIGHT_EFFECT, tick) {
            let phase = (time / LIGHT_PHASE_SECONDS) as usize % LIGHT_PHASES.len();
            let colour = LIGHT_PHASES[phase];
            let tint = Color32::from_rgba_unmultiplied(
                colour.r(),
                colour.g(),
                colour.b(),
                (alpha * 255.0).round() as u8,
            );
            draw_pivoted_tinted(ui, stage, &lights, WINDOW_ORIGIN, tint);
        }
    }

    /// The four characters: drawn over the backdrop and, while they are still
    /// unmasked, with the glow that marks the special one.
    ///
    /// Returns the slot of the character the player clicked, if any.
    fn draw_characters(
        &self,
        ui: &Ui,
        textures: &mut Textures,
        stage: &Stage,
        time: f64,
    ) -> Option<usize> {
        let elapsed = time - self.phase_started;
        let (pose, tick) = self.pose(time, elapsed);
        let clicked = characters::draw(
            ui,
            textures,
            stage,
            &self.characters,
            characters::Step {
                pose,
                tick,
                walked: (elapsed / WALK_SECONDS).clamp(0.0, 1.0) as f32,
                pickable: self.phase == Phase::Pick,
            },
        );

        if self.phase == Phase::Reveal {
            self.draw_glow(ui, textures, stage, time);
        }

        clicked
    }

    /// The glow ringing the special character.
    ///
    /// Placed by its pivot, so the artwork moves exactly as it was authored: see
    /// [`layout::glow_pivot`].
    fn draw_glow(&self, ui: &Ui, textures: &mut Textures, stage: &Stage, time: f64) {
        let tick = (time * GLOW_FPS) as usize;
        let Some(glow) = textures.frame(ui.ctx(), assets::EMPHASIS, tick) else {
            return;
        };
        let stance = characters::special_stance(&self.characters);
        draw_pivoted_scaled(
            ui,
            stage,
            &glow,
            layout::glow_pivot(stance.anchor, stance.scale),
            stance.scale,
        );
    }

    /// The `READY` stamp, which plays once inside the stage.
    fn draw_ready_stamp(&self, ui: &Ui, textures: &mut Textures, stage: &Stage, time: f64) {
        if self.phase != Phase::Ready {
            return;
        }
        let tick = ((time - self.phase_started) * READY_FPS) as usize;
        if let Some(stamp) = textures.frame_once(ui.ctx(), assets::READY_STAMP, tick) {
            draw_pivoted(ui, stage, &stamp, WINDOW_ORIGIN);
        }
    }

    /// The banners that play over the whole window: `START`, and then whichever
    /// of success and fail the game ended on.
    fn draw_banner_animation(&self, ui: &Ui, textures: &mut Textures, stage: &Stage, time: f64) {
        let group = match self.phase {
            Phase::Start => assets::START_BANNER,
            Phase::Success => assets::SUCCESS_BANNER,
            Phase::Failed => assets::FAIL_BANNER,
            _ => return,
        };

        let tick = ((time - self.phase_started) * BANNER_FPS) as usize;
        if let Some(banner) = textures.frame_once(ui.ctx(), group, tick) {
            draw_pivoted(ui, stage, &banner, WINDOW_ORIGIN);
        }
    }

    /// The window frame: drawn over the stage, so it masks it.
    fn draw_window(&self, ui: &Ui, textures: &mut Textures, stage: &Stage) {
        if let Some(frame) = textures.get(ui.ctx(), assets::WINDOW_FRAME) {
            draw_pivoted(ui, stage, &frame, WINDOW_ORIGIN);
        }
    }

    /// The device's timer, which runs while the player is choosing.
    ///
    /// The gauge is drawn into the track the window's artwork already has under
    /// the readout, drained towards its right-hand end the way the game's does,
    /// with the seconds left counted down beside it.
    fn draw_timer(&self, ui: &Ui, textures: &mut Textures, stage: &Stage, time: f64) {
        if self.phase != Phase::Pick {
            return;
        }
        let elapsed = time - self.phase_started;
        let left = (1.0 - elapsed / ANSWER_SECONDS).clamp(0.0, 1.0) as f32;

        let (left_x, top, right_x, bottom) = layout::TIMER_TRACK;
        // The gauge is drained from its right-hand end, so what is left of it stays
        // against the left of the track.
        let gauge = Rect::from_min_max(
            stage.point(left_x, top),
            stage.point(left_x + (right_x - left_x) * left, bottom),
        );
        ui.painter().rect_filled(gauge, 0.0, TIMER_FILL);

        let seconds = (ANSWER_SECONDS - elapsed).clamp(1.0, ANSWER_SECONDS).ceil() as usize;
        let Some(name) = assets::frame_of(assets::TIMER_DIGIT, seconds) else {
            return;
        };
        let Some(digit) = textures.get(ui.ctx(), name) else {
            return;
        };
        draw_scaled(ui, stage, &digit, layout::TIMER_CENTRE, layout::TIMER_SCALE);
    }

    /// The digits the device shows: the position the player picked in each round.
    fn draw_readout(&self, ui: &Ui, textures: &mut Textures, stage: &Stage) {
        for (index, pick) in self.picks.iter().enumerate() {
            let Some(position) = *pick else { continue };
            // The readout's frames are the digits themselves: frame 1 is a `1`.
            let Some(name) = assets::frame_of(assets::READOUT_DIGIT, position as usize) else {
                continue;
            };
            let Some(digit) = textures.get(ui.ctx(), name) else {
                continue;
            };
            draw_pivoted(ui, stage, &digit, layout::readout_pivot(index));
        }
    }

    /// The answer buttons, which commit the player to a position.
    ///
    /// `clicked_character` is the slot the player clicked on the stage itself,
    /// which counts as pressing that button.
    fn draw_buttons(
        &mut self,
        ui: &mut Ui,
        textures: &mut Textures,
        stage: &Stage,
        time: f64,
        clicked_character: Option<usize>,
        verification: Option<&Verification>,
    ) {
        // Answers are only taken while the round is waiting for one.
        let enabled = self.phase == Phase::Pick;
        let mut chosen = clicked_character.filter(|_| enabled);

        for index in 0..ANSWERS {
            if widgets::answer_button(ui, stage, textures, index, enabled).clicked() {
                chosen = Some(index);
            }
        }

        if let Some(slot) = chosen {
            self.pick(slot, time, verification);
        }
    }
}

impl Default for GameScreen {
    fn default() -> Self {
        Self::new()
    }
}

/// Decodes the artwork a round draws, so that no phase stalls on PNG decoding.
fn preload(ctx: &Context, textures: &mut Textures) {
    let groups = [
        assets::LIGHT_EFFECT,
        assets::EMPHASIS,
        assets::READY_STAMP,
        assets::START_BANNER,
        assets::SUCCESS_BANNER,
        assets::FAIL_BANNER,
        assets::CHARACTER_IDLE,
        assets::CHARACTER_MASK_ON,
        assets::CHARACTER_MASK_OFF,
        assets::SPECIAL_CHARACTER_IDLE,
        assets::SPECIAL_CHARACTER_MASK_ON,
        assets::SPECIAL_CHARACTER_MASK_OFF,
        assets::CHARACTER_WALK,
        assets::STANDBY_DIGIT,
        assets::READOUT_DIGIT,
        assets::TIMER_DIGIT,
    ];

    let mut names = vec![
        assets::STAGE_BACKDROP.to_owned(),
        assets::WINDOW_FRAME.to_owned(),
        assets::STANDBY_DIALOG.to_owned(),
        assets::MESSAGE_WATCH.to_owned(),
        assets::MESSAGE_PICK.to_owned(),
        assets::MESSAGE_ANSWER.to_owned(),
    ];
    for group in groups {
        names.extend(
            assets::frames(group)
                .unwrap_or_default()
                .iter()
                .map(|frame| (*frame).to_owned()),
        );
    }
    for index in 0..ANSWERS {
        for state in widgets::BUTTON_STATES {
            names.push(widgets::button_sprite(index, state));
        }
    }

    for name in &names {
        textures.get(ctx, name);
    }
}

/// The verification clock, in the top-left corner of the page.
///
/// It is the page's own chrome rather than part of the window: the game has no such
/// display, and it is the deadline the bot holds the member to. The seconds are read
/// off the Worker's clock — anchored when the session was last fetched — so a device
/// whose own clock is wrong still counts down correctly.
fn draw_verification(ui: &Ui, verification: &Verification, time: f64) {
    let remaining = verification.remaining(time).max(0.0);
    let seconds = remaining.ceil() as u64;
    let clock = format!("{}:{:02}", seconds / 60, seconds % 60);

    // The device readout's amber, going red in the last minute.
    let colour = if remaining <= 60.0 {
        Color32::from_rgb(0xff, 0x6b, 0x6b)
    } else {
        Color32::from_rgb(0xff, 0xd2, 0x00)
    };

    let tries = verification
        .snapshot()
        .map_or(0, |snapshot| snapshot.attempts_left);
    let tries = match tries {
        1 => "1 try left".to_owned(),
        count => format!("{count} tries left"),
    };

    Area::new(Id::new("verification-clock"))
        .anchor(Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
        .show(ui.ctx(), |ui| {
            Frame::NONE
                .fill(Color32::from_black_alpha(0xa0))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(clock).size(26.0).strong().color(colour));
                        ui.label(
                            RichText::new(tries)
                                .size(12.0)
                                .color(Color32::from_gray(0xc9)),
                        );
                    });
                });
        });
}

/// Draws the instruction line in the strip along the top of the frame.
///
/// The line is the game's own artwork rather than text this app composes, so it
/// comes out in the original lettering wherever the app is run. It is drawn at
/// its own size, centred on the strip like the game centres it.
fn draw_banner(ui: &Ui, textures: &mut Textures, stage: &Stage, message: &str) {
    let Some(sprite) = textures.get(ui.ctx(), message) else {
        return;
    };
    draw_scaled(ui, stage, &sprite, layout::BANNER_CENTRE, 1.0);
}

/// How long `group` takes to play once at `fps`.
fn seconds_of(group: &str, fps: f64) -> f64 {
    assets::frames(group).map_or(0.0, |frames| frames.len() as f64 / fps)
}
