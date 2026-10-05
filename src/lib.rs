pub mod app;
pub mod config;
pub mod editor;
pub mod net;
pub mod panels;
pub mod rewire_integration;
pub mod state;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
#[wasm_bindgen]
pub struct PerseWebHandle {
    runner: eframe::WebRunner,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl PerseWebHandle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        eframe::WebLogger::init(re_log::external::log::LevelFilter::Debug).ok();
        Self {
            runner: eframe::WebRunner::new(),
        }
    }

    #[wasm_bindgen]
    pub async fn start(
        &self,
        canvas: web_sys::HtmlCanvasElement,
        rrd_url: Option<String>,
        _theme: Option<String>,
    ) -> Result<(), JsValue> {
        let connect = rrd_url.unwrap_or_else(|| {
            let host = web_sys::window()
                .and_then(|w| w.location().hostname().ok())
                .unwrap_or_else(|| "127.0.0.1".to_string());
            format!("rerun+http://{}:9876/proxy", host)
        });
        self.runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| Ok(Box::new(app::MyApp::new(cc, connect, None)?))),
            )
            .await
    }

    #[wasm_bindgen]
    pub fn has_panicked(&self) -> bool {
        self.runner.has_panicked()
    }

    #[wasm_bindgen]
    pub fn panic_message(&self) -> Option<String> {
        self.runner.panic_summary().map(|s| s.message())
    }
}
