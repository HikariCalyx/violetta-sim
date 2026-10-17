//! Result screen: how the game ended, and a way to start another.

use egui::{Align, Button, CentralPanel, Color32, Frame, Layout, RichText, Ui};

use crate::language::Words;
use crate::screens::{BACKGROUND, CHAOS_FILL, CHAOS_GAP, Mode, Outcome};

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
/// Clearing the normal setting also offers chaos mode, which is where a player who
/// has just won is most likely to want it: they are already being asked to play
/// again. The button is the title screen's, in the same red and the same distance
/// below the other one.
pub fn show(ui: &mut Ui, outcome: Outcome, words: Words, mode: Mode) -> Option<Next> {
    let (headline, colour) = match outcome {
        Outcome::Success => (words.success, Color32::from_rgb(0xff, 0xd2, 0x00)),
        Outcome::Failure => (words.failed, Color32::from_rgb(0xff, 0x6b, 0x6b)),
    };
    // Chaos is what a cleared normal game leads on to, so it is offered there and
    // nowhere else: a game already played at chaos has no harder setting to offer,
    // and a loss is not a thing to make harder.
    let offers_chaos = outcome == Outcome::Success && mode == Mode::Normal;

    let mut next = None;

    CentralPanel::default()
        .frame(Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.28);

                ui.label(RichText::new(headline).size(40.0).strong().color(colour));

                ui.add_space(48.0);

                let button = Button::new(RichText::new(words.try_again).size(22.0))
                    .min_size(egui::vec2(180.0, 52.0));
                if ui.add(button).clicked() {
                    next = Some(Next::Same);
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
