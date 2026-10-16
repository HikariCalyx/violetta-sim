//! Maps the fixed 441x318 window artwork onto the screen.

use egui::{Color32, Pos2, Rect, TextureHandle, Ui, Vec2};

use crate::assets::Sprite;

use super::layout;

/// The anchor for sprites that the game's UI data positions itself: their pivot
/// is recorded relative to the window's top-left corner.
pub const WINDOW_ORIGIN: (f32, f32) = (0.0, 0.0);

/// The transform from window space to screen space.
#[derive(Debug, Clone, Copy)]
pub struct Stage {
    origin: Pos2,
    scale: f32,
}

impl Stage {
    /// Centres the window in `screen`.
    ///
    /// When the window would be enlarged, the scale is snapped down to a whole
    /// number so that the pixel art stays crisp and square; on a small screen it
    /// shrinks by the fraction that fits.
    pub fn new(screen: Rect) -> Self {
        let fits = (screen.width() / layout::WINDOW.x).min(screen.height() / layout::WINDOW.y);
        let scale = if fits >= 1.0 {
            fits.floor()
        } else {
            fits.max(0.0)
        };

        Self {
            origin: screen.center() - layout::WINDOW * scale / 2.0,
            scale,
        }
    }

    /// Whether there is any room to draw the window at all.
    pub fn is_visible(&self) -> bool {
        self.scale > 0.0
    }

    /// The scale factor from window space to screen space.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// A point in window space, on screen.
    pub fn point(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.scale
    }

    /// A rectangle in window space, on screen.
    pub fn rect(&self, x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(self.point(x, y), Vec2::new(width, height) * self.scale)
    }
}

/// The rectangle `sprite` covers when its pivot is placed on `anchor`, a point in
/// window space.
///
/// Sprites are placed by their pivot rather than by their top-left corner because
/// every frame's image is trimmed to its own drawing; the pivot, which comes from
/// the game's UI data, is what holds an animation still.
pub fn pivot_rect(stage: &Stage, sprite: &Sprite, anchor: (f32, f32)) -> Rect {
    let size = sprite.size();
    stage.rect(
        anchor.0 - sprite.origin.x,
        anchor.1 - sprite.origin.y,
        size.x,
        size.y,
    )
}

/// Draws `sprite` with its canvas centred on `centre`, enlarged by `scale` image
/// pixels to a window pixel, and returns the rectangle it covers.
///
/// The enlargement is for artwork that is drawn larger than it was authored — the
/// timer's digit is a readout glyph blown up — and is meant to be a whole number,
/// which keeps pixel art crisp under nearest-neighbour sampling.
pub fn draw_scaled(
    ui: &Ui,
    stage: &Stage,
    sprite: &Sprite,
    centre: (f32, f32),
    scale: f32,
) -> Rect {
    draw_scaled_tinted(ui, stage, sprite, centre, scale, Color32::WHITE)
}

/// Draws `sprite` with its canvas centred on `centre`, enlarged by `scale` image
/// pixels to a window pixel, and tinted by `tint`.
///
/// The tint carries the strength for artwork that is shown faintly, which is what
/// the stage lamps' scattered figures are drawn with.
pub fn draw_scaled_tinted(
    ui: &Ui,
    stage: &Stage,
    sprite: &Sprite,
    centre: (f32, f32),
    scale: f32,
    tint: Color32,
) -> Rect {
    let size = sprite.size() * scale * stage.scale();
    let rect = Rect::from_center_size(stage.point(centre.0, centre.1), size);
    blit_tinted(ui, &sprite.texture, rect, tint);
    rect
}

/// Draws `sprite` with its pivot on `anchor`, and returns the rectangle it covers.
pub fn draw_pivoted(ui: &Ui, stage: &Stage, sprite: &Sprite, anchor: (f32, f32)) -> Rect {
    draw_pivoted_scaled(ui, stage, sprite, anchor, 1.0)
}

/// Draws `sprite` with its pivot on `anchor`, shrunk by `scale`, and returns the
/// rectangle it covers.
///
/// Scaling keeps the pivot where it is and shrinks towards it, which is what the
/// stage's perspective needs: a character further back sits higher *and* is
/// smaller, so the size has to follow the row it is standing in.
pub fn draw_pivoted_scaled(
    ui: &Ui,
    stage: &Stage,
    sprite: &Sprite,
    anchor: (f32, f32),
    scale: f32,
) -> Rect {
    let size = sprite.size() * scale * stage.scale();
    let rect = Rect::from_min_size(
        stage.point(
            anchor.0 - sprite.origin.x * scale,
            anchor.1 - sprite.origin.y * scale,
        ),
        size,
    );
    blit(ui, &sprite.texture, rect);
    rect
}

/// Draws `sprite` with its pivot on `anchor`, tinted by `tint`.
///
/// The stage lights are the artwork at reduced opacity, which is a tint of white
/// with an alpha.
pub fn draw_pivoted_tinted(
    ui: &Ui,
    stage: &Stage,
    sprite: &Sprite,
    anchor: (f32, f32),
    tint: Color32,
) -> Rect {
    let rect = pivot_rect(stage, sprite, anchor);
    blit_tinted(ui, &sprite.texture, rect, tint);
    rect
}

/// Draws `texture` stretched to fill `rect`.
pub fn blit(ui: &Ui, texture: &TextureHandle, rect: Rect) {
    blit_tinted(ui, texture, rect, Color32::WHITE);
}

/// Draws `texture` stretched to fill `rect`, tinted by `tint`.
pub fn blit_tinted(ui: &Ui, texture: &TextureHandle, rect: Rect, tint: Color32) {
    egui::Image::new(egui::load::SizedTexture::new(texture.id(), rect.size()))
        .tint(tint)
        .paint_at(ui, rect);
}
