//! Result screen: how the game ended, and — for a plain game — a way to start
//! another.

use egui::{Align, Button, CentralPanel, Color32, Frame, Layout, RichText, Ui};

use crate::language::Words;
use crate::net::Verification;
use crate::screens::{BACKGROUND, CHAOS_FILL, CHAOS_GAP, Mode, Outcome};

/// The amber the game's own readout uses for a win.
const GOLD: Color32 = Color32::from_rgb(0xff, 0xd2, 0x00);
/// The red it uses for a loss.
const RED: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
/// The grey of the second line, which explains what the headline means.
const GREY: Color32 = Color32::from_gray(0xc9);

/// What the player asked to play next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// The setting the game that just ended was played at.
    Same,
    /// Chaos mode, which a game cleared at the normal setting offers.
    Chaos,
}

/// Draws the outcome in the words the app is being played in, and reports what the
/// player asked to play next, if anything.
///
/// A verification adds the second line and takes the button away when there is
/// nothing left to try: the Worker is what decides whether the member may play
/// again, and once it has removed them there is no game to restart. The button
/// replays with the shuffle the Worker sent for the next attempt, which arrives
/// with the report that ended the last one.
///
/// Clearing the normal setting also offers chaos mode, which is where a player who
/// has just won is most likely to want it: they are already being asked to play
/// again. The button is the title screen's, in the same red and the same distance
/// below the other one. A verification never offers it — the Worker chooses the
/// setting there, and its link already brings chaos when that is what it wants.
pub fn show(
    ui: &mut Ui,
    outcome: Outcome,
    verification: Option<&Verification>,
    words: Words,
    mode: Mode,
) -> Option<Next> {
    let tries_left = verification
        .and_then(|verification| verification.snapshot())
        .map(|snapshot| snapshot.attempts_left);
    let tries = tries_left.map(|left| words.tries_left(left));

    let (headline, detail, colour, retry) = match (outcome, tries_left) {
        // No session: the public simulator, which can always be played again.
        (Outcome::Success, None) => (words.success, None, GOLD, true),
        (Outcome::Failure, None) => (words.failed, None, RED, true),
        (Outcome::Success, Some(_)) => (words.verified, Some(words.verified_detail), GOLD, false),
        (Outcome::Expired, _) => (words.time_up, Some(words.removed), RED, false),
        (Outcome::Failure, Some(0)) => (words.out_of_tries, Some(words.removed), RED, false),
        (Outcome::Failure, Some(_)) => (words.failed, tries.as_deref(), RED, true),
    };
    // Chaos is what a cleared normal game leads on to, so it is offered there and
    // nowhere else: a game already played at chaos has no harder setting to offer, a
    // verification is played at the setting the Worker chose, and a loss is not a
    // thing to make harder.
    let offers_chaos =
        verification.is_none() && outcome == Outcome::Success && mode == Mode::Normal;

    let mut next = None;

    CentralPanel::default()
        .frame(Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.28);

                ui.label(RichText::new(headline).size(40.0).strong().color(colour));

                if let Some(detail) = detail {
                    ui.add_space(14.0);
                    ui.label(RichText::new(detail).size(15.0).color(GREY));
                }

                if retry {
                    ui.add_space(48.0);

                    let button = Button::new(RichText::new(words.try_again).size(22.0))
                        .min_size(egui::vec2(180.0, 52.0));
                    if ui.add(button).clicked() {
                        next = Some(Next::Same);
                    }
                }

                if offers_chaos {
                    ui.add_space(CHAOS_GAP);

                    let chaos = Button::new(
                        RichText::new(words.try_chaos)
                            .size(22.0)
                            .color(Color32::WHITE),
                    )
                    .min_size(egui::vec2(180.0, 52.0))
                    .fill(CHAOS_FILL);
                    if ui.add(chaos).clicked() {
                        next = Some(Next::Chaos);
                    }
                }
            });
        });

    next
}
