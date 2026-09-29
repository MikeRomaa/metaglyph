//! The engine behind the web editor (plan 6, W1). The editor runs this in a
//! Web Worker: every document change goes through [`Engine::update`], which
//! reparses, lowers and evaluates the whole file and returns a [`DocState`]
//! the UI renders from.
//!
//! Offsets crossing this boundary are UTF-16 code units, the unit
//! CodeMirror (and every JS string) counts in; the crates count bytes.

mod doc;
mod offsets;

pub use doc::{DiagnosticInfo, DocState, FontInfo, check};
use wasm_bindgen::prelude::*;

/// One open document. Holds nothing yet but the version it last saw; the
/// CST and HIR move in here once edit ops need them (plan 6, W3).
#[wasm_bindgen]
#[derive(Default)]
pub struct Engine {
    version: u32,
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
        self.version = version;
        let state = check(source, version);
        Ok(serde_wasm_bindgen::to_value(&state)?)
    }
}
