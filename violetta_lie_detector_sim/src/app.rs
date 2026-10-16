//! Application state and the top-level screen switch.

use std::sync::Arc;

use egui::Ui;

use crate::assets::{self, Textures};
use crate::language;
use crate::screens::{self, Outcome, game::GameScreen};

/// Which screen the app is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Title screen, showing the *Start* button.
    Start,
    /// The lie detector window itself.
    Game,
    /// How the game that just finished ended.
    Result(Outcome),
}

/// Root of the app: owns the shared state and dispatches to the screens.
pub struct LieDetectorApp {
    screen: Screen,
    game: GameScreen,
    textures: Textures,
}

impl LieDetectorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // The UI is drawn on top of its own artwork, so do not follow the
        // browser's/desktop's light or dark preference. The artwork is read from
        // the language the platform is set to, and only the title screen's
        // photograph is decoded here, because it is what the first frame shows;
        // the rest is loaded while the countdown is on screen.
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        let mut textures = Textures::detect();
        textures.get_smooth(&cc.egui_ctx, assets::START_BACKGROUND);
        install_name_fonts(&cc.egui_ctx);
        set_title(&cc.egui_ctx, language::title(textures.language()));

        Self {
            screen: Screen::Start,
            game: GameScreen::new(),
            textures,
        }
    }

    /// Starts a new game. The seed comes from the clock, so no two games lay the
    /// characters out the same way.
    fn start_game(&mut self, ui: &Ui) {
        let time = ui.input(|input| input.time);
        self.game.restart(ui.ctx(), &mut self.textures, time);
        self.screen = Screen::Game;
    }
}

impl eframe::App for LieDetectorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.screen {
            Screen::Start => {
                if screens::start::show(ui, &mut self.textures) {
                    self.start_game(ui);
                }
            }
            Screen::Game => {
                if let Some(outcome) = self.game.show(ui, &mut self.textures) {
                    self.screen = Screen::Result(outcome);
                }
            }
            Screen::Result(outcome) => {
                let words = language::words(self.textures.language());
                if screens::result::show(ui, outcome, words) {
                    self.start_game(ui);
                }
            }
        }
    }
}

/// Adds the fonts the localised names are drawn with.
///
/// egui's own fonts have no CJK, and a name it cannot draw is a row of boxes. Each
/// of these is a subset holding one language's name (see [`assets::LANGUAGE_FONTS`]),
/// and they go on the end of the families rather than at the front: a glyph is
/// looked up in order, so Latin keeps the font the rest of the app uses and only
/// the CJK falls through to these.
fn install_name_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    for (language, bytes) in assets::LANGUAGE_FONTS {
        fonts.font_data.insert(
            (*language).to_owned(),
            Arc::new(egui::FontData::from_static(bytes)),
        );

        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push((*language).to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

/// Puts the app's name where the platform shows it: the browser tab when running
/// in a browser, the window title on the desktop.
///
/// The desktop can be told through egui, but a web build cannot — eframe's web
/// runner does not act on the viewport title — so the browser gets it from the
/// document directly. Either way the name in `index.html` and in `main.rs` is only
/// what shows before the app starts.
fn set_title(ctx: &egui::Context, title: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title(title);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_owned()));
}
