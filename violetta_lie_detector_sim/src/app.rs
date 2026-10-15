//! Application state and the top-level screen switch.

use egui::Ui;

use crate::assets::{self, Textures};
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
        // browser's/desktop's light or dark preference. Only the title screen's
        // photograph is decoded here, because it is what the first frame shows;
        // the rest of the artwork is loaded while the countdown is on screen.
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        let mut textures = Textures::default();
        textures.get_smooth(&cc.egui_ctx, assets::START_BACKGROUND);

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
                if screens::result::show(ui, outcome) {
                    self.start_game(ui);
                }
            }
        }
    }
}
