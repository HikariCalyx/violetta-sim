//! The sprite-drawn widgets of the detector window.

use egui::{CursorIcon, Id, Rect, Response, Sense, Ui};

use crate::assets::Textures;

use super::stage::{Stage, WINDOW_ORIGIN, draw_pivoted, pivot_rect};

/// The artwork states of an answer button.
const STATE_NORMAL: &str = "normal";
const STATE_HOVERED: &str = "mouseOver";
const STATE_PRESSED: &str = "pressed";
const STATE_DISABLED: &str = "disabled";

/// Every state of an answer button, for preloading its artwork.
pub const BUTTON_STATES: [&str; 4] = [STATE_NORMAL, STATE_HOVERED, STATE_PRESSED, STATE_DISABLED];

/// Draws answer button `index` and returns its interaction response.
///
/// The button is assembled from the `lieDetector.buttonselectN` sprites, so the
/// hover and pressed artwork comes from the original game rather than from egui's
/// own widget style. All four states share a pivot, so the button does not shift
/// as it is pointed at.
///
/// The sprite group is built from a `format!` each frame; that is a handful of
/// small allocations per frame, which is fine here and keeps the naming in one
/// place.
pub fn answer_button(
    ui: &mut Ui,
    stage: &Stage,
    textures: &mut Textures,
    index: usize,
    enabled: bool,
) -> Response {
    let id = Id::new(("answer-button", index));
    let Some(normal) = textures.frame(ui.ctx(), &button_sprite(index, STATE_NORMAL), 0) else {
        return ui.interact(Rect::NOTHING, id, Sense::hover());
    };

    let rect = pivot_rect(stage, &normal, WINDOW_ORIGIN);
    // A live button takes clicks; a disabled one only senses the pointer, so
    // callers cannot be handed a click in a phase that is not accepting answers.
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, id, sense);

    let state = if !enabled {
        STATE_DISABLED
    } else if response.is_pointer_button_down_on() {
        STATE_PRESSED
    } else if response.hovered() {
        STATE_HOVERED
    } else {
        STATE_NORMAL
    };

    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let sprite = if state == STATE_NORMAL {
        normal
    } else {
        textures
            .frame(ui.ctx(), &button_sprite(index, state), 0)
            .unwrap_or(normal)
    };
    draw_pivoted(ui, stage, &sprite, WINDOW_ORIGIN);

    response
}

/// The sprite group holding the artwork for `state` of answer button `index`.
pub fn button_sprite(index: usize, state: &str) -> String {
    format!("lieDetector.buttonselect{}.{state}", index + 1)
}
