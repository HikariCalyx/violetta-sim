//! The sprite library: what artwork exists, and the textures it is drawn with.
//!
//! `build.rs` embeds every PNG in `Resources/` (and the title screen's JPEG) and
//! indexes the animation frames, so a sprite is named by its file name without the
//! extension (`lieDetector.back1`), and one frame of an animation by `<name>.<frame>`
//! (`lieDetector.npc.stand.3`). Adding artwork needs no code change here.

use std::collections::HashMap;
use std::sync::OnceLock;

use egui::{ColorImage, Context, TextureHandle, TextureOptions, Vec2};

include!(concat!(env!("OUT_DIR"), "/sprites.rs"));

/// The window frame, with its title bar, instruction strip and device.
pub const WINDOW_FRAME: &str = "lieDetector.back1";
/// The theatre stage seen through the window.
pub const STAGE_BACKDROP: &str = "lieDetector.back";
/// The stage lights, drawn over the stage once the masks are on.
pub const LIGHT_EFFECT: &str = "lieDetector.lightEffect";
/// The glow that marks the character the player is pointing at.
pub const EMPHASIS: &str = "lieDetector.gameEffect.emphasis";
/// The `READY` stamp played before a round.
pub const READY_STAMP: &str = "lieDetector.gameEffect.ready";
/// The `START` banner that opens a round.
pub const START_BANNER: &str = "lieDetector.gameEffect.start";
/// The banner played when every attempt was right.
pub const SUCCESS_BANNER: &str = "lieDetector.gameEffect.success";
/// The banner played when an attempt was wrong.
pub const FAIL_BANNER: &str = "lieDetector.gameEffect.fail";
/// The instruction shown while the characters are on the stage: keep an eye on
/// the one in make-up.
pub const MESSAGE_WATCH: &str = "lieDetector.textMessage.0";
/// The instruction shown while a position is being picked.
pub const MESSAGE_PICK: &str = "lieDetector.textMessage.2";
/// The line shown as the answer is given, warning of the macro accusation.
pub const MESSAGE_ANSWER: &str = "lieDetector.textMessage.1";
/// A character standing in the open.
pub const CHARACTER_IDLE: &str = "lieDetector.npc.stand";
/// A character putting the mask on.
pub const CHARACTER_MASK_ON: &str = "lieDetector.npc.maskOn";
/// A character taking the mask off.
pub const CHARACTER_MASK_OFF: &str = "lieDetector.npc.maskOff";
/// The special character standing in the open.
pub const SPECIAL_CHARACTER_IDLE: &str = "lieDetector.npcSpecial.stand";
/// The special character putting the mask on.
pub const SPECIAL_CHARACTER_MASK_ON: &str = "lieDetector.npcSpecial.maskOn";
/// The special character taking the mask off, which gives it away again.
pub const SPECIAL_CHARACTER_MASK_OFF: &str = "lieDetector.npcSpecial.maskOff";
/// The walk cycle every character shares while the masks are on.
pub const CHARACTER_WALK: &str = "lieDetector.npcCommon.move";
/// The digits of the detector's readout.
pub const READOUT_DIGIT: &str = "lieDetector.number1";
/// The digits of the device's timer, the second of the two digit sets.
pub const TIMER_DIGIT: &str = "lieDetector.number2";
/// The dialog the game shows while it waits for a game to start. The countdown
/// before the window opens is shown on it.
pub const STANDBY_DIALOG: &str = "lieDetectorStandBy.back";
/// The digits of that dialog, whose instruction line leaves a blank for them.
pub const STANDBY_DIGIT: &str = "lieDetectorStandBy.number";
/// The photograph behind the title screen, a still from the video the screen
/// credits. It is not pixel art: it is scaled to cover the screen, so it is drawn
/// with linear filtering rather than with the nearest-neighbour the sprites use.
pub const START_BACKGROUND: &str = "bg";

/// The frames of animation `group`, in index order.
///
/// Groups are named after the sprites that make them up, so the frames of the
/// idling animation are `frames(CHARACTER_IDLE)`.
pub fn frames(group: &str) -> Option<&'static [&'static str]> {
    frame_index().get(group).copied()
}

/// Sprite `index` of animation `group`.
pub fn frame_of(group: &str, index: usize) -> Option<&'static str> {
    frames(group)?.get(index).copied()
}

/// A decoded sprite: its texture, and the pivot its drawing hangs from.
///
/// The pivot is where the sprite's `origin` sits, in image pixels. Each frame of
/// an animation is trimmed to its own drawing, so placing a frame by its pivot
/// rather than by its corner is what stops an animation from shaking.
#[derive(Clone)]
pub struct Sprite {
    /// The decoded image.
    pub texture: TextureHandle,
    /// Pivot within the image, in image pixels.
    pub origin: Vec2,
}

impl Sprite {
    /// The size of the image, in image pixels.
    pub fn size(&self) -> Vec2 {
        self.texture.size_vec2()
    }
}

/// Looks up sprite names.
fn index() -> &'static HashMap<&'static str, usize> {
    static INDEX: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    INDEX.get_or_init(|| {
        SPRITES
            .iter()
            .enumerate()
            .map(|(index, (name, ..))| (*name, index))
            .collect()
    })
}

/// Looks up animation groups.
fn frame_index() -> &'static HashMap<&'static str, &'static [&'static str]> {
    static INDEX: OnceLock<HashMap<&'static str, &'static [&'static str]>> = OnceLock::new();
    INDEX.get_or_init(|| FRAME_GROUPS.iter().copied().collect())
}

/// Decodes sprites on demand and keeps their textures for the app's lifetime.
#[derive(Default)]
pub struct Textures {
    loaded: HashMap<&'static str, TextureHandle>,
}

impl Textures {
    /// Returns sprite `name`, decoding it on first use.
    ///
    /// Returns `None` for a name that is not in `Resources/`, which is a
    /// programming mistake rather than a runtime condition: it is reported once
    /// and the caller draws nothing.
    pub fn get(&mut self, ctx: &Context, name: &str) -> Option<Sprite> {
        self.load(ctx, name, TextureOptions::NEAREST)
    }

    /// Returns sprite `name`, decoded on first use with linear filtering.
    ///
    /// For artwork that is resized to fit rather than drawn at its own size, which
    /// is everything the pixel art is not: the title screen's photograph would
    /// show aliasing if it were sampled nearest-neighbour.
    pub fn get_smooth(&mut self, ctx: &Context, name: &str) -> Option<Sprite> {
        self.load(ctx, name, TextureOptions::LINEAR)
    }

    /// Decodes `name` with `options` on first use, and keeps it for the app's life.
    fn load(&mut self, ctx: &Context, name: &str, options: TextureOptions) -> Option<Sprite> {
        let Some(&index) = index().get(name) else {
            log_once(name);
            return None;
        };
        let (key, bytes, (pivot_x, pivot_y)) = SPRITES[index];

        let texture = match self.loaded.get(key) {
            Some(texture) => texture.clone(),
            None => {
                let texture = ctx.load_texture(key, decode(key, bytes), options);
                self.loaded.insert(key, texture.clone());
                texture
            }
        };

        Some(Sprite {
            texture,
            origin: Vec2::new(pivot_x as f32, pivot_y as f32),
        })
    }

    /// Returns frame `tick` of animation `group`, wrapping around at its end.
    ///
    /// `tick` is a frame counter, not a time: callers pick the rate so that a
    /// sprite can be played at, say, one frame per 120 ms.
    pub fn frame(&mut self, ctx: &Context, group: &str, tick: usize) -> Option<Sprite> {
        let keys = frames(group)?;
        let key = keys.get(tick % keys.len())?;
        self.get(ctx, key)
    }

    /// Returns frame `tick` of animation `group`, holding its last frame once the
    /// animation has played out.
    ///
    /// For animations that play once — the stamps, the masks and the banners —
    /// looping back to the first frame would visibly restart them.
    pub fn frame_once(&mut self, ctx: &Context, group: &str, tick: usize) -> Option<Sprite> {
        let keys = frames(group)?;
        let key = keys.get(tick.min(keys.len().saturating_sub(1)))?;
        self.get(ctx, key)
    }

    /// Returns the last frame of animation `group`, which is the pose a one-shot
    /// animation settles into.
    pub fn last_frame(&mut self, ctx: &Context, group: &str) -> Option<Sprite> {
        let keys = frames(group)?;
        self.get(ctx, keys.last()?)
    }
}

/// Reports a missing sprite once per name, so a typo does not spam the log.
fn log_once(name: &str) {
    static REPORTED: OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        OnceLock::new();
    let reported = REPORTED.get_or_init(Default::default);
    let mut reported = reported.lock().expect("sprite report lock");
    if reported.insert(name.to_owned()) {
        log::warn!("no sprite named `{name}` in Resources/");
    }
}

/// Decodes a sprite into an image egui can upload as a texture.
///
/// The format is whatever the file turns out to be, so the sprites' PNGs and the
/// title screen's JPEG both go through here. Panics if the file cannot be decoded:
/// artwork is baked in at compile time, so a failure here is a broken build rather
/// than a runtime condition.
fn decode(name: &str, bytes: &[u8]) -> ColorImage {
    let decoded = image::load_from_memory(bytes)
        .unwrap_or_else(|err| panic!("failed to decode `{name}`: {err}"))
        .to_rgba8();

    let size = [decoded.width() as usize, decoded.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, decoded.as_raw())
}
