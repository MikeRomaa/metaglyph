//! The engine behind the web editor (plan 6). The editor runs this in a
//! Web Worker: every document change goes through [`Engine::update`],
//! which reparses, lowers and evaluates the whole file; the views then
//! query the result per instance with [`Engine::font_data`] and
//! [`Engine::glyph_scene`].
//!
//! Offsets crossing this boundary are UTF-16 code units, the unit
//! CodeMirror (and every JS string) counts in; the crates count bytes.

mod doc;
mod offsets;
mod view;

pub use doc::{DiagnosticInfo, DocState, FontInfo, Model, analyze, check};
pub use view::{FontData, GlyphScene, font_data, format_num, glyph_scene};
use wasm_bindgen::prelude::*;

/// One open document.
#[wasm_bindgen]
#[derive(Default)]
pub struct Engine {
    /// The last text that parsed. Kept while the text has syntax errors,
    /// so the views show the last good state (plan 5, §1.1).
    model: Option<Model>,
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Engine {
        Engine::default()
    }

    /// Checks `source`, tagging the result with `version` so the caller can
    /// drop a result computed for text it has since changed.
    pub fn update(&mut self, source: &str, version: u32) -> Result<JsValue, JsError> {
        let (state, model) = analyze(source, version);
        if model.is_some() {
            self.model = model;
        }
        Ok(serde_wasm_bindgen::to_value(&state)?)
    }

    /// [`FontData`] for `instance`, from the last good text; `null` before
    /// any text parsed or for an unknown instance.
    #[wasm_bindgen(js_name = fontData)]
    pub fn font_data(&self, instance: &str) -> Result<JsValue, JsError> {
        let data = self.model.as_ref().and_then(|m| font_data(m, instance));
        Ok(serde_wasm_bindgen::to_value(&data)?)
    }

    /// [`GlyphScene`] for `glyph` in `instance`, from the last good text.
    #[wasm_bindgen(js_name = glyphScene)]
    pub fn glyph_scene(&self, instance: &str, glyph: &str) -> Result<JsValue, JsError> {
        let scene = self
            .model
            .as_ref()
            .and_then(|m| glyph_scene(m, instance, glyph));
        Ok(serde_wasm_bindgen::to_value(&scene)?)
    }
}
