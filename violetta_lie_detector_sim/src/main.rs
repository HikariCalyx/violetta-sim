//! Violetta Lie Detector Simulator.
//!
//! One code base, two targets: a wasm app for the browser (built with `trunk`)
//! and a native binary (built with `cargo run`) for fast iteration.

mod app;
mod assets;
mod net;
mod screens;

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([489.0, 350.0])
            .with_title("Violetta Lie Detector Simulator"),
        ..Default::default()
    };

    eframe::run_native(
        "Violetta Lie Detector Simulator",
        native_options,
        Box::new(|cc| Ok(Box::new(app::LieDetectorApp::new(cc)))),
    )
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` messages to the browser console:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("failed to find `the_canvas_id`")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("`the_canvas_id` was not a canvas");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(app::LieDetectorApp::new(cc)))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(()) => loading_text.remove(),
                Err(err) => {
                    loading_text.set_inner_html(
                        "<p>The app has crashed. See the developer console for details.</p>",
                    );
                    panic!("failed to start eframe: {err:?}");
                }
            }
        }
    });
}
