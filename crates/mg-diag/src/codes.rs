/// A stable, documented, greppable diagnostic code (spec §13).
///
/// Codes are grouped by class: `MG01xx` syntax, `MG02xx` name resolution,
/// `MG03xx` type checking, `MG04xx` field validation, `MG05xx` path
/// structure, `MG06xx` evaluation and domain errors, `MG07xx` geometry,
/// `MG08xx` export. Each milestone adds its codes here; a shipped code is
/// never renumbered or reused for a different error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Code(&'static str);

impl Code {
    pub const fn new(code: &'static str) -> Self {
        Code(code)
    }

    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
