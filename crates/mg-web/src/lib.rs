//! The engine behind the web editor (plan 6). The editor runs this in a
//! Web Worker: every document change goes through [`Engine::update`],
//! which reparses, lowers and evaluates the whole file; the views then
//! query the result per instance with [`Engine::font_data`] and
//! [`Engine::glyph_scene`].
//!
//! Offsets crossing this boundary are UTF-16 code units, the unit
//! CodeMirror (and every JS string) counts in; the crates count bytes.

mod build;
mod complete;
mod doc;
mod drag;
mod offsets;
mod ops;
mod view;

pub use build::build;
pub use complete::{Completion, complete};
pub use doc::{DiagnosticInfo, DocState, FontInfo, Model, analyze, check};
pub use ops::{Change, EditResult, Op};
use std::rc::Rc;
pub use view::{FontData, GlyphScene, font_data, format_num, glyph_scene};

use wasm_bindgen::prelude::*;

/// Sends `log` records to the browser console, and panics through them,
/// so a panic reports its message and location. Once per module instance.
fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        wasm_logger::init(wasm_logger::Config::default());
        std::panic::set_hook(Box::new(|info| log::error!("{info}")));
    });
}

/// Milliseconds on the JS clock, for timing calls in debug logs:
/// `std::time::Instant` is unavailable on `wasm32-unknown-unknown`. Only
/// called from [`Engine`] methods, which only run in the browser.
fn now() -> f64 {
    js_sys::Date::now()
}

/// `value` for JS with `None` as `null`, not `undefined`: the drag results'
/// types say `null` (an axis with no driver), and the editor compares
/// with `=== null`. Other results keep `undefined` for absent fields.
fn to_js_nulls<T: serde::Serialize>(value: &T) -> Result<JsValue, serde_wasm_bindgen::Error> {
    value.serialize(&serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true))
}

/// One open document.
#[wasm_bindgen]
#[derive(Default)]
pub struct Engine {
    /// The last text that parsed. Kept while the text has syntax errors,
    /// so the views show the last good state (plan 5, §1.1).
    model: Option<Rc<Model>>,
    /// The newest text, parsed or not, and its version: edit ops run
    /// against exactly this.
    current: Option<(String, u32)>,
    /// A text pinned by [`Engine::pin`]: an edit gesture's start. Its
    /// edits run against it while the gesture moves the text, so each is
    /// the net change from the start.
    anchor: Option<(String, u32)>,
    /// The drag in progress. It keeps the drag-start evaluation, so each
    /// step (which changes the text) solves against the same state.
    drag: Option<drag::Session>,
    /// The driver chosen per axis, per point, for this session.
    prefs: drag::Preferences,
    /// Name types of the last text completion saw lower, for member
    /// completion while the text has errors.
    types: Option<mg_lsp::types::NameTypes>,
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Engine {
        init_logging();
        log::info!(
            "mg-web {} ({} build) started",
            env!("CARGO_PKG_VERSION"),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        );
        Engine::default()
    }

    /// Checks `source`, tagging the result with `version` so the caller can
    /// drop a result computed for text it has since changed.
    pub fn update(&mut self, source: &str, version: u32) -> Result<JsValue, JsError> {
        let start = now();
        let (state, model) = analyze(source, version);
        if let Some(model) = model {
            self.model = Some(Rc::new(model));
        }
        self.current = Some((source.to_string(), version));
        log::debug!(
            "update v{version}: {} bytes, parsed: {}, evaluated: {}, {} diagnostics, {:.1} ms",
            source.len(),
            state.parse_ok,
            state.evaluated,
            state.diagnostics.len(),
            now() - start,
        );
        Ok(serde_wasm_bindgen::to_value(&state)?)
    }

    /// The [`Completion`]s at UTF-16 `offset` in `source`.
    pub fn complete(&mut self, source: &str, offset: usize) -> Result<JsValue, JsError> {
        let items = complete(source, offset, &mut self.types);
        Ok(serde_wasm_bindgen::to_value(&items)?)
    }

    /// [`FontData`] for `instance`, from the last good text; `null` before
    /// any text parsed or for an unknown instance.
    #[wasm_bindgen(js_name = fontData)]
    pub fn font_data(&self, instance: &str) -> Result<JsValue, JsError> {
        let start = now();
        let data = self.model.as_deref().and_then(|m| font_data(m, instance));
        match &data {
            Some(d) => log::debug!(
                "font data {instance}: {} glyphs, {} kerns, {:.1} ms",
                d.glyphs.len(),
                d.kerns.len(),
                now() - start,
            ),
            None => log::debug!("font data {instance}: none (no model, or no such instance)"),
        }
        Ok(serde_wasm_bindgen::to_value(&data)?)
    }

    /// Runs an edit op (an [`Op`] as JSON) against document `version`,
    /// returning an [`EditResult`]: the changes to apply, or why not.
    pub fn edit(&self, op: JsValue, version: u32) -> Result<JsValue, JsError> {
        let start = now();
        let op: Op = serde_wasm_bindgen::from_value(op)?;
        let result = match (&self.current, &self.anchor) {
            (Some((source, v)), _) | (_, Some((source, v))) if *v == version => {
                ops::run(source, version, &op)
            }
            _ => EditResult::Stale,
        };
        let outcome = match &result {
            EditResult::Ok { steps, created, .. } => format!(
                "ok, {} step(s){}",
                steps.len(),
                created
                    .as_ref()
                    .map(|c| format!(", created {} {}", c.kind, c.name))
                    .unwrap_or_default(),
            ),
            EditResult::Stale => format!(
                "stale (engine is at v{})",
                self.current.as_ref().map_or(0, |(_, v)| *v)
            ),
            EditResult::ReadOnly => "read-only (syntax errors)".to_string(),
            EditResult::Invalid { message } => format!("invalid: {message}"),
        };
        log::debug!("edit v{version} {op:?}: {outcome}, {:.1} ms", now() - start);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Pins document `version` for an edit gesture (a guide or metric
    /// drag): [`Engine::edit`] keeps accepting `version` until
    /// [`Engine::unpin`]. False if `version` isn't the current text.
    pub fn pin(&mut self, version: u32) -> bool {
        self.anchor = self.current.clone().filter(|(_, v)| *v == version);
        self.anchor.is_some()
    }

    pub fn unpin(&mut self) {
        self.anchor = None;
    }

    /// Builds every instance as TTF (plan 6, W8) with `head` timestamps of
    /// `timestamp` (seconds since the Unix epoch): `{ fonts: [{ instance,
    /// fileName, data: Uint8Array }], diagnostics }`. `null` when the
    /// current text isn't the one that last evaluated (it has errors).
    #[wasm_bindgen(js_name = buildTtf)]
    pub fn build_ttf(&self, timestamp: f64) -> Result<JsValue, JsError> {
        let model = match (&self.model, &self.current) {
            (Some(model), Some((text, _))) if *text == model.source => model,
            _ => return Ok(JsValue::NULL),
        };
        let start = now();
        let (fonts, diagnostics) = build::build(model, timestamp as i64);
        log::debug!(
            "build: {} font(s), {} diagnostic(s), {:.1} ms",
            fonts.len(),
            diagnostics.len(),
            now() - start,
        );
        let list = js_sys::Array::new();
        for font in &fonts {
            let entry = js_sys::Object::new();
            js_sys::Reflect::set(&entry, &"instance".into(), &font.instance.as_str().into())
                .map_err(|_| JsError::new("could not build the result"))?;
            js_sys::Reflect::set(&entry, &"fileName".into(), &font.file_name.as_str().into())
                .map_err(|_| JsError::new("could not build the result"))?;
            let data = js_sys::Uint8Array::from(font.data.as_slice());
            js_sys::Reflect::set(&entry, &"data".into(), &data)
                .map_err(|_| JsError::new("could not build the result"))?;
            list.push(&entry);
        }
        let result = js_sys::Object::new();
        js_sys::Reflect::set(&result, &"fonts".into(), &list)
            .map_err(|_| JsError::new("could not build the result"))?;
        js_sys::Reflect::set(
            &result,
            &"diagnostics".into(),
            &serde_wasm_bindgen::to_value(&diagnostics)?,
        )
        .map_err(|_| JsError::new("could not build the result"))?;
        Ok(result.into())
    }

    /// [`GlyphScene`] for `glyph` in `instance`, from the last good text.
    #[wasm_bindgen(js_name = glyphScene)]
    pub fn glyph_scene(&self, instance: &str, glyph: &str) -> Result<JsValue, JsError> {
        let start = now();
        let scene = self
            .model
            .as_deref()
            .and_then(|m| glyph_scene(m, instance, glyph));
        match &scene {
            Some(s) => log::debug!(
                "scene {instance}/{glyph}: {} paths, {} points, {} lines, {:.1} ms",
                s.paths.len(),
                s.points.len(),
                s.lines.len(),
                now() - start,
            ),
            None => log::debug!("scene {instance}/{glyph}: none"),
        }
        Ok(serde_wasm_bindgen::to_value(&scene)?)
    }

    /// What dragging point `target` would change (the DRIVERS panel);
    /// `null` if it can't be dragged.
    pub fn drivers(&self, instance: &str, glyph: &str, target: &str) -> Result<JsValue, JsError> {
        let info = self.model.clone().and_then(|model| {
            let prefer = self.preference(glyph, target);
            drag::Session::begin(model, instance, glyph, target, prefer)
                .map_err(|e| log::debug!("drivers {glyph}/{target}: {e}"))
                .ok()
                .map(|s| s.info())
        });
        Ok(to_js_nulls(&info)?)
    }

    /// Starts dragging point `target` in document `version`: the drag's
    /// edits apply to that version's text. `null` if the text isn't the
    /// last one that evaluated (so it can't be solved against) or the
    /// point has no value.
    #[wasm_bindgen(js_name = dragBegin)]
    pub fn drag_begin(
        &mut self,
        instance: &str,
        glyph: &str,
        target: &str,
        version: u32,
    ) -> Result<JsValue, JsError> {
        self.drag = None;
        let current = match (&self.model, &self.current) {
            (Some(model), Some((text, v))) if *v == version && *text == model.source => {
                Some(model.clone())
            }
            _ => {
                log::debug!("drag {glyph}/{target} v{version}: text is not the evaluated one");
                None
            }
        };
        let prefer = self.preference(glyph, target);
        let session = current.and_then(|model| {
            drag::Session::begin(model, instance, glyph, target, prefer)
                .map_err(|e| log::debug!("drag {glyph}/{target}: {e}"))
                .ok()
        });
        let info = session.as_ref().map(|s| s.info());
        if let Some(info) = &info {
            log::debug!(
                "drag {glyph}/{target} v{version}: {} driver(s), axes {:?}",
                info.drivers.len(),
                info.axis
            );
        }
        self.drag = session;
        Ok(to_js_nulls(&info)?)
    }

    /// The drag step for the pointer at `(x, y)`; `null` without a drag.
    #[wasm_bindgen(js_name = dragTo)]
    pub fn drag_to(&self, x: f64, y: f64) -> Result<JsValue, JsError> {
        let start = now();
        let step = self.drag.as_ref().map(|s| s.drag_to([x, y]));
        if let Some(step) = &step {
            log::debug!(
                "drag to ({x:.1}, {y:.1}): {:?}, exact: {}, {:.1} ms",
                step.literals,
                step.exact,
                now() - start
            );
        }
        Ok(to_js_nulls(&step)?)
    }

    /// The drag step for setting driver `driver` to `value` (the scrub
    /// slider); `null` without a drag.
    #[wasm_bindgen(js_name = dragSet)]
    pub fn drag_set(&self, driver: usize, value: f64) -> Result<JsValue, JsError> {
        let step = self.drag.as_ref().map(|s| s.set(driver, value));
        Ok(to_js_nulls(&step)?)
    }

    /// Cycles axis `axis`'s driver (0: x, 1: y) and returns the new
    /// [`drag::DragInfo`]; `null` without a drag.
    #[wasm_bindgen(js_name = dragCycle)]
    pub fn drag_cycle(&mut self, axis: usize) -> Result<JsValue, JsError> {
        let info = self.drag.as_mut().map(|s| {
            s.cycle(axis.min(1));
            s.info()
        });
        Ok(to_js_nulls(&info)?)
    }

    /// Ends the drag, remembering its driver choice for the point.
    #[wasm_bindgen(js_name = dragEnd)]
    pub fn drag_end(&mut self, glyph: &str) {
        if let Some(session) = self.drag.take() {
            let info = session.info();
            self.prefs
                .insert((glyph.to_string(), info.target.clone()), info.axis);
            log::debug!("drag {glyph}/{} ended", info.target);
        }
    }
}

impl Engine {
    fn preference(&self, glyph: &str, target: &str) -> [Option<usize>; 2] {
        self.prefs
            .get(&(glyph.to_string(), target.to_string()))
            .copied()
            .unwrap_or([None, None])
    }
}
