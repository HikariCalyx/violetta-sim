//! Result screen: how the game ended, and a way to start another.

use egui::{Align, Button, CentralPanel, Color32, Frame, Layout, RichText, Ui};

use crate::screens::{BACKGROUND, Outcome};

/// Draws the outcome and reports whether *Try Again* was clicked.
pub fn show(ui: &mut Ui, outcome: Outcome) -> bool {
    let (headline, colour) = match outcome {
        Outcome::Success => ("Success", Color32::from_rgb(0xff, 0xd2, 0x00)),
        Outcome::Failure => ("Failed", Color32::from_rgb(0xff, 0x6b, 0x6b)),
    };

    let mut try_again = false;

    CentralPanel::default()
        .frame(Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.28);

                ui.label(RichText::new(headline).size(40.0).strong().color(colour));

                ui.add_space(48.0);

                let button = Button::new(RichText::new("Try Again").size(22.0))
                    .min_size(egui::vec2(180.0, 52.0));
                if ui.add(button).clicked() {
                    try_again = true;
                }
            });
        });

    try_again
}
