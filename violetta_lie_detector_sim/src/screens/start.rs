//! Title screen: the background photograph, a *Start* button that takes the player
//! into the game, and the credit for the photograph.

use egui::{
    Align, Align2, Area, Button, CentralPanel, Color32, CornerRadius, Frame, Id, Layout, Margin,
    Rect, RichText, Ui, Vec2, pos2,
};

use crate::assets::{self, Textures};
use crate::language;
use crate::screens::{BACKGROUND, CHAOS_FILL, CHAOS_GAP, Mode};

/// The video the background photograph is a still from, which the credit links to.
const CREDIT_URL: &str = "https://www.youtube.com/watch?v=K7MTgHeFrYc";
/// The credit, up to the name that the link is made of.
const CREDIT: &str = "Background Image Credit:";
/// How far the credit sits from the bottom-left corner of the screen.
const CREDIT_MARGIN: f32 = 10.0;

/// The repository the icon button in the top-right corner opens.
const REPOSITORY: &str = "https://github.com/HikariCalyx/violetta-sim";
/// How large the icon button's logo is drawn.
const REPOSITORY_ICON: f32 = 22.0;
/// How far the icon button sits from the top-right corner of the screen.
const REPOSITORY_MARGIN: f32 = 10.0;

/// The English name of the second button, which a language without a name of its
/// own keeps.
const CHAOS: &str = "Chaos Mode";

/// Draws the title screen and reports which of its buttons was clicked.
pub fn show(ui: &mut Ui, textures: &mut Textures) -> Option<Mode> {
    let mut chosen = None;

    CentralPanel::default()
        .frame(Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            draw_photograph(ui, textures);

            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.28);

                ui.label(
                    RichText::new(language::title(textures.language()))
                        .size(40.0)
                        .strong()
                        .color(Color32::from_rgb(0xff, 0xd2, 0x00)),
                );

                ui.add_space(48.0);

                let start = Button::new(
                    RichText::new(language::words(textures.language()).start).size(22.0),
                )
                .min_size(egui::vec2(180.0, 52.0));
                if ui.add(start).clicked() {
                    chosen = Some(Mode::Normal);
                }

                ui.add_space(CHAOS_GAP);

                let label = language::words(textures.language()).chaos.unwrap_or(CHAOS);
                let chaos = Button::new(RichText::new(label).size(22.0).color(Color32::WHITE))
                    .min_size(egui::vec2(180.0, 52.0))
                    .fill(CHAOS_FILL);
                if ui.add(chaos).clicked() {
                    chosen = Some(Mode::Chaos);
                }
            });
        });

    draw_credit(ui);
    draw_repository(ui);

    chosen
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

/// The link to the repository the app is built from, in the top-right corner.
///
/// It carries the GitHub logo rather than a name, because the app's own name is
/// already on the screen, and it sits on the credit's translucent plate for the same
/// reason that one does: a photograph can be any colour. The logo is a glyph of the
/// icon font egui bundles ([`egui::special_emojis`]); the app only appends the CJK
/// fonts to the default ones (`app::install_name_fonts`), so this glyph is still the
/// one drawn.
fn draw_repository(ui: &Ui) {
    Area::new(Id::new("repository-link"))
        .anchor(
            Align2::RIGHT_TOP,
            Vec2::new(-REPOSITORY_MARGIN, REPOSITORY_MARGIN),
        )
        .show(ui.ctx(), |ui| {
            let icon = RichText::new(egui::special_emojis::GITHUB)
                .size(REPOSITORY_ICON)
                .color(Color32::from_gray(0xe8));
            let button = Button::new(icon)
                .fill(Color32::from_black_alpha(0xa0))
                .corner_radius(CornerRadius::same(4));
            let response = ui.add(button).on_hover_text(REPOSITORY);
            if response.clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(REPOSITORY));
            }
        });
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
