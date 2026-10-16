//! Pixel geometry of the detector window.
//!
//! Coordinates are in *window space*: the origin is the top-left corner of
//! `lieDetector.back1.png`, and the window measures 441x318 px.
//!
//! Most of the window needs no coordinates at all. The game's UI data records a
//! pivot for every image (see `build.rs`), and a sprite drawn with its pivot on
//! `WINDOW_ORIGIN` lands exactly where the game draws it: the window frame, the
//! stage backdrop, the light effect, the `READY` stamp and the four answer
//! buttons all place themselves that way.
//!
//! Only the sprites that the game positions from its own code need an anchor
//! here — the characters, the glow and the readout digits. Those were fitted
//! against the reference footage in `Reference/`, which is a 1:1 recording of the
//! window cropped 19 px below its top edge and shifted 5 px on x.

use egui::Vec2;

/// The window artwork.
pub const WINDOW: Vec2 = Vec2::new(441.0, 318.0);

/// The opening in the window frame, as `(left, top, width, height)`: everything
/// that happens on the stage is seen through it, and the frame — drawn last —
/// masks whatever leaves it.
pub const OPENING: (f32, f32, f32, f32) = (15.0, 67.0, 412.0, 187.0);

/// How many positions the stage has.
pub const ANSWER_COUNT: usize = 4;

/// Centre of the window, which is also the centre of the screen: the countdown
/// runs there before the window opens.
pub const SCREEN_CENTRE: (f32, f32) = (WINDOW.x / 2.0, WINDOW.y / 2.0);
/// Where the countdown's number goes on the standby dialog, whose origin is its
/// top-left corner. The dialog's instruction line reads "… sec until the Lie
/// Detector mini-game starts.", and leaves a blank before the `sec` for the
/// number to sit in.
pub const STANDBY_COUNT: (f32, f32) = (31.0, 29.5);

/// The instruction strip along the top of the frame.
pub const BANNER_TOP: f32 = 25.0;
pub const BANNER_HEIGHT: f32 = 31.0;
/// Centre of the instruction strip, where the message is drawn.
pub const BANNER_CENTRE: (f32, f32) = (220.5, BANNER_TOP + BANNER_HEIGHT / 2.0);

/// Characters stand in four evenly spaced slots, one above each answer button.
pub const SLOT_PIVOT_X: f32 = 44.0;
pub const SLOT_STEP: f32 = 104.0;
/// Where a character's pivot sits, measured from the top of the window. It is
/// what lands the character's feet on the stage floor at y = 208.
pub const CHARACTER_PIVOT_Y: f32 = 121.0;
/// The row the characters stand in: the front of the stage, where
/// [`CHARACTER_PIVOT_Y`] puts their feet on the floor. The game lines all four of
/// them up there — at the reveal, and again for the picks — so the rows behind it
/// are only ever walked through.
pub const FRONT_ROW: usize = 0;
/// How far upstage each further row stands: the stage is drawn in perspective, so
/// a row back is also higher up the window.
///
/// The step stays small because most of the distance is told by the shrinking
/// below, and because the backdrop only leaves about 90 px of floor behind the
/// front row: a bigger step would lift the deepest of them off the floor and onto
/// the steps at the back of the stage.
pub const DEPTH_STEP: f32 = 9.0;
/// How much smaller a character is drawn for each row further back — a fifth a row,
/// so the deepest of the four is two fifths the size of the one in front.
///
/// It sounds like a lot, and it is on purpose: while the masks are on the four are
/// identical, so their size and their height are all that tells them apart when
/// their paths cross.
pub const DEPTH_SCALE: f32 = 0.8;
/// The row each character is sent through by a shuffle: every row behind the front
/// one, one each, so no two of them are ever at the same depth on the way.
pub const WALK_ROWS: [usize; ANSWER_COUNT] = [1, 2, 3, 4];
/// A character's drawing is centred this far right of and below its pivot, i.e.
/// on its body. It is what turns a slot's pivot into the point a character is
/// centred on.
pub const CHARACTER_CENTRE: Vec2 = Vec2::new(19.0, 43.0);

/// The glow's ring is centred this far right of and below the emphasis artwork's
/// pivot, averaged over its frames.
///
/// Measured by fitting a circle to the ring in each of the 16 frames: they agree
/// to within about three pixels, so the pivot is a dependable anchor and the
/// animation can be left to move exactly as it was authored.
pub const GLOW_RING_FROM_PIVOT: Vec2 = Vec2::new(45.0, 54.0);

/// The detector's readout: four digit cells whose pivots sit below-right of the
/// glyphs.
pub const READOUT_PIVOT_X: f32 = 201.0;
pub const READOUT_PIVOT_Y: f32 = 99.0;
pub const READOUT_STEP: f32 = 21.4;

/// The gauge track the window's artwork draws under the readout, as
/// `(left, top, right, bottom)`. The frame already has the track and its clock
/// icon; what the game adds is the fill, which drains as time runs out.
pub const TIMER_TRACK: (f32, f32, f32, f32) = (197.0, 94.5, 259.0, 100.5);
/// Where the seconds remaining are centred, and how much the digit is enlarged.
/// The digit is a 9x13 readout glyph, so the enlargement is a whole number, which
/// keeps it crisp.
pub const TIMER_CENTRE: (f32, f32) = (222.0, 113.0);
pub const TIMER_SCALE: f32 = 3.0;

/// Pivot of character slot `index`.
pub fn slot_pivot_x(index: usize) -> f32 {
    SLOT_PIVOT_X + index as f32 * SLOT_STEP
}

/// Pivot of the glow that rings a character anchored at `anchor` and drawn at
/// `scale`, which is the character's own scale so the ring stays the size of the
/// character it rings.
///
/// The glow is placed by its pivot and not by centring the frame's canvas. The
/// ring moves around inside the loosely trimmed canvases (110 to 190 px wide),
/// so centring the canvas adds that wander on top of the animation's own motion
/// and the ring visibly jitters; anchoring the pivot leaves only the authoring.
/// The offset is what puts the ring's centre on the character's body.
pub fn glow_pivot(anchor: (f32, f32), scale: f32) -> (f32, f32) {
    (
        anchor.0 + (CHARACTER_CENTRE.x - GLOW_RING_FROM_PIVOT.x) * scale,
        anchor.1 + (CHARACTER_CENTRE.y - GLOW_RING_FROM_PIVOT.y) * scale,
    )
}

/// Pivot of readout cell `index`.
pub fn readout_pivot(index: usize) -> (f32, f32) {
    (
        READOUT_PIVOT_X + index as f32 * READOUT_STEP,
        READOUT_PIVOT_Y,
    )
}
