//! Title screen: the background photograph, a *Start* button that takes the player
//! into the game, and the credit for the photograph.

use egui::{
    Align, Align2, Area, Button, CentralPanel, Color32, CornerRadius, Frame, Id, Layout, Margin,
    Rect, RichText, Ui, Vec2, pos2,
};

use crate::assets::{self, Textures};
use crate::screens::BACKGROUND;

/// The video the background photograph is a still from, which the credit links to.
const CREDIT_URL: &str = "https://www.youtube.com/watch?v=K7MTgHeFrYc";
/// The credit, up to the name that the link is made of.
const CREDIT: &str = "Background Image Credit:";
/// How far the credit sits from the bottom-left corner of the screen.
const CREDIT_MARGIN: f32 = 10.0;

/// Draws the title screen and reports whether *Start* was clicked.
pub fn show(ui: &mut Ui, textures: &mut Textures) -> bool {
    let mut start_clicked = false;

    CentralPanel::default()
        .frame(Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            draw_photograph(ui, textures);

            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.28);

                ui.label(
                    RichText::new("Violetta Lie Detector Simulator")
                        .size(40.0)
                        .strong()
                        .color(Color32::from_rgb(0xff, 0xd2, 0x00)),
                );

                ui.add_space(48.0);

                let start = Button::new(RichText::new("Start").size(22.0))
                    .min_size(egui::vec2(180.0, 52.0));
                if ui.add(start).clicked() {
                    start_clicked = true;
                }
            });
        });

    draw_credit(ui);

    start_clicked
}

/// Fills the screen with the background photograph.
///
/// It is scaled to cover and then cropped to the screen, so it keeps the shape it
/// was taken in instead of being stretched to whatever shape the window is.
fn draw_photograph(ui: &Ui, textures: &mut Textures) {
    let Some(photograph) = textures.get_smooth(ui.ctx(), assets::START_BACKGROUND) else {
        return;
    };

    let screen = ui.max_rect();
    let size = photograph.size();
    let scale = (screen.width() / size.x).max(screen.height() / size.y);
    let rect = Rect::from_center_size(screen.center(), size * scale);

    ui.painter().with_clip_rect(screen).image(
        photograph.texture.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// The credit for the photograph, in the bottom-left corner, with the name linked
/// to the video it was taken from.
///
/// It sits on a translucent plate because a photograph can be any colour and the
/// credit has to stay readable on it.
fn draw_credit(ui: &Ui) {
    Area::new(Id::new("background-credit"))
        .anchor(
            Align2::LEFT_BOTTOM,
            Vec2::new(CREDIT_MARGIN, -CREDIT_MARGIN),
        )
        .show(ui.ctx(), |ui| {
            Frame::NONE
                .fill(Color32::from_black_alpha(0xa0))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(CREDIT)
                                .size(13.0)
                                .color(Color32::from_gray(0xd0)),
                        );
                        ui.hyperlink_to(RichText::new("Somacguffin").size(13.0), CREDIT_URL);
                    });
                });
        });
}
