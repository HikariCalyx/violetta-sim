//! Application state and the top-level screen switch.

use std::sync::Arc;

use egui::{Align, CentralPanel, Color32, Frame, Layout, RichText, Ui};

use crate::assets::{self, Textures};
use crate::language::{self, Words};
use crate::net::{self, Verification};
use crate::screens::{self, BACKGROUND, Mode, Outcome, game::GameScreen};

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
    /// The mode the current game was started in, which *Try Again* keeps.
    mode: Mode,
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
        // browser's/desktop's light or dark preference. The artwork, and the words
        // the screens write themselves, are read from the language the platform is
        // set to.
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        // A link the bot hands out carries the session it belongs to. Without one
        // this is the plain simulator, which is also what the public demo is.
        let verification = net::session_id().map(Verification::open);

        let mut textures = Textures::detect();
        // The title screen's photograph is what the first frame shows, so it is
        // decoded here; the rest of the artwork is loaded while the countdown is on
        // screen. A verification never shows that screen, so it is not decoded.
        if verification.is_none() {
            textures.get_smooth(&cc.egui_ctx, assets::START_BACKGROUND);
        }
        install_name_fonts(&cc.egui_ctx);
        set_title(&cc.egui_ctx, language::title(textures.language()));

        Self {
            // A verification does not start at the title screen: the member is muted
            // and their ten minutes are already running.
            screen: if verification.is_some() {
                Screen::Game
            } else {
                Screen::Start
            },
            mode: Mode::Normal,
            game: GameScreen::new(),
            textures,
            verification,
            started: false,
        }
    }

    /// Starts a new game in `mode`. The seed comes from the clock, so no two games
    /// lay the characters out the same way.
    fn start_game(&mut self, ui: &Ui, mode: Mode) {
        let time = ui.input(|input| input.time);
        self.game.restart(ui.ctx(), &mut self.textures, time, mode);
        self.mode = mode;
        self.started = true;
        self.screen = Screen::Game;
    }

    /// Starts the verification's game once the Worker has handed out its shuffle.
    ///
    /// Returns whether there is a game to show. While the session is still being read
    /// there is nothing to play, and starting a game of our own would walk the player
    /// through shuffles the Worker never scored.
    fn start_verified(&mut self, ui: &mut Ui, time: f64) -> bool {
        let words = language::words(self.textures.language());
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
            draw_waiting(ui, verification, words);
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
                if let Some(mode) = screens::start::show(ui, &mut self.textures) {
                    self.start_game(ui, mode);
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
                let words = language::words(self.textures.language());
                let again = screens::result::show(ui, outcome, self.verification.as_ref(), words);
                if again {
                    // A verification replays with the shuffle the Worker sent for the
                    // next attempt, which arrives with the report that ended the last
                    // one — so the game is started again from the session.
                    self.started = false;
                    self.screen = Screen::Game;
                    if self.verification.is_none() {
                        // A retry stays at the setting the game was played at.
                        self.start_game(ui, self.mode);
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
/// session, and the member's clock is running while they look at it. The reason the
/// Worker gave is appended to it as it stands, since it names what went wrong.
fn draw_waiting(ui: &mut Ui, verification: &Verification, words: Words) {
    let (message, colour) = match verification.error() {
        Some(error) => (
            format!("{}\n{error}", words.unavailable),
            Color32::from_rgb(0xff, 0x6b, 0x6b),
        ),
        None => (words.checking.to_owned(), Color32::from_gray(0xc9)),
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
