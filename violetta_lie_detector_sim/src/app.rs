//! Application state and the top-level screen switch.

use egui::{Align, CentralPanel, Color32, Frame, Layout, RichText, Ui};

use crate::assets::{self, Textures};
use crate::net::{self, Verification};
use crate::screens::{self, BACKGROUND, Outcome, game::GameScreen};

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
    /// The verification this page was opened for, when the bot's link brought the
    /// player here. Without one the app is the plain simulator it has always been.
    verification: Option<Verification>,
    /// Whether the game on screen has already been started from the session's shuffle.
    started: bool,
}

impl LieDetectorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // The UI is drawn on top of its own artwork, so do not follow the
        // browser's/desktop's light or dark preference.
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        // A link the bot hands out carries the session it belongs to. Without one
        // this is the plain simulator, which is also what the public demo is.
        let verification = net::session_id().map(Verification::open);

        let mut textures = Textures::default();
        // The title screen's photograph is what the first frame shows, so it is
        // decoded here; the rest of the artwork is loaded while the countdown is on
        // screen. A verification never shows that screen, so it is not decoded.
        if verification.is_none() {
            textures.get_smooth(&cc.egui_ctx, assets::START_BACKGROUND);
        }

        Self {
            // A verification does not start at the title screen: the member is muted
            // and their ten minutes are already running.
            screen: if verification.is_some() {
                Screen::Game
            } else {
                Screen::Start
            },
            game: GameScreen::new(),
            textures,
            verification,
            started: false,
        }
    }

    /// Starts a new game. The seed comes from the clock, so no two games lay the
    /// characters out the same way.
    fn start_game(&mut self, ui: &Ui) {
        let time = ui.input(|input| input.time);
        self.game.restart(ui.ctx(), &mut self.textures, time);
        self.started = true;
        self.screen = Screen::Game;
    }

    /// Starts the verification's game once the Worker has handed out its shuffle.
    ///
    /// Returns whether there is a game to show. While the session is still being read
    /// there is nothing to play, and starting a game of our own would walk the player
    /// through shuffles the Worker never scored.
    fn start_verified(&mut self, ui: &mut Ui, time: f64) -> bool {
        let Some(verification) = self.verification.as_ref() else {
            return true;
        };

        if !self.started {
            if let Some(plan) = verification.plan() {
                self.game
                    .restart_verified(ui.ctx(), &mut self.textures, time, &plan);
                self.started = true;
            }
        }

        if !self.started {
            draw_waiting(ui, verification);
            // The first snapshot may arrive at any moment, so keep drawing.
            ui.ctx().request_repaint();
        }

        self.started
    }
}

impl eframe::App for LieDetectorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let time = ui.input(|input| input.time);
        if let Some(verification) = self.verification.as_mut() {
            verification.refresh(time);
        }

        match self.screen {
            Screen::Start => {
                if screens::start::show(ui, &mut self.textures) {
                    self.start_game(ui);
                }
            }
            Screen::Game => {
                if !self.start_verified(ui, time) {
                    return;
                }
                let verification = self.verification.as_ref();
                if let Some(outcome) = self.game.show(ui, &mut self.textures, verification) {
                    self.started = false;
                    self.screen = Screen::Result(outcome);
                }
            }
            Screen::Result(outcome) => {
                let again = screens::result::show(ui, outcome, self.verification.as_ref());
                if again {
                    // A verification replays with the shuffle the Worker sent for the
                    // next attempt, which arrives with the report that ended the last
                    // one — so the game is started again from the session.
                    self.started = false;
                    self.screen = Screen::Game;
                    if self.verification.is_none() {
                        self.start_game(ui);
                    }
                }
            }
        }
    }
}

/// What the page shows while the session is still being read, or when it could not
/// be read at all.
///
/// The second case is worth spelling out on screen: the page is useless without its
/// session, and the member's clock is running while they look at it.
fn draw_waiting(ui: &mut Ui, verification: &Verification) {
    let (message, colour) = match verification.error() {
        Some(error) => (
            format!("This verification is unavailable.\n{error}"),
            Color32::from_rgb(0xff, 0x6b, 0x6b),
        ),
        None => (
            "Checking your verification…".to_owned(),
            Color32::from_gray(0xc9),
        ),
    };

    CentralPanel::default()
        .frame(Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.4);
                ui.label(RichText::new(message).size(19.0).color(colour));
            });
        });
}
