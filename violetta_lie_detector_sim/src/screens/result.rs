//! Result screen: how the game ended, and — for a plain game — a way to start
//! another.

use egui::{Align, Button, CentralPanel, Color32, Frame, Layout, RichText, Ui};

use crate::language::Words;
use crate::net::Verification;
use crate::screens::{BACKGROUND, Outcome};

/// The amber the game's own readout uses for a win.
const GOLD: Color32 = Color32::from_rgb(0xff, 0xd2, 0x00);
/// The red it uses for a loss.
const RED: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
/// The grey of the second line, which explains what the headline means.
const GREY: Color32 = Color32::from_gray(0xc9);

/// Draws the outcome in the words the app is being played in, and reports whether
/// *Try Again* was clicked.
///
/// A verification adds the second line and takes the button away when there is
/// nothing left to try: the Worker is what decides whether the member may play
/// again, and once it has removed them there is no game to restart. The button
/// replays with the shuffle the Worker sent for the next attempt, which arrives
/// with the report that ended the last one.
///
/// Only the two outcomes the simulator itself reports — *Success* and *Failed* —
/// are in `language::words`; the rest belong to the bot, and are English.
pub fn show(
    ui: &mut Ui,
    outcome: Outcome,
    verification: Option<&Verification>,
    words: Words,
) -> bool {
    let tries_left = verification
        .and_then(|verification| verification.snapshot())
        .map(|snapshot| snapshot.attempts_left);
    let tries = tries_left.map(|left| match left {
        1 => "1 try left.".to_owned(),
        count => format!("{count} tries left."),
    });

    let (headline, detail, colour, retry) = match (outcome, tries_left) {
        // No session: the public simulator, which can always be played again.
        (Outcome::Success, None) => (words.success, None, GOLD, true),
        (Outcome::Failure, None) => (words.failed, None, RED, true),
        (Outcome::Success, Some(_)) => (
            "Verified",
            Some("You can talk in the group now."),
            GOLD,
            false,
        ),
        (Outcome::Expired, _) => (
            "Time is up",
            Some("You were removed from the group. You can rejoin and try again."),
            RED,
            false,
        ),
        (Outcome::Failure, Some(0)) => (
            "Out of tries",
            Some("You were removed from the group. You can rejoin and try again."),
            RED,
            false,
        ),
        (Outcome::Failure, Some(_)) => (words.failed, tries.as_deref(), RED, true),
    };

    let mut try_again = false;

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
                        try_again = true;
                    }
                }
            });
        });

    try_again
}
