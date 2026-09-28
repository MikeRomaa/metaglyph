use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mg_diag::Diagnostic;
use mg_diag::termcolor::{ColorChoice, StandardStream};
use mg_syntax::ast::AstNode;

/// Prints `diagnostic` to `stream`, colored when `stream`'s own
/// `ColorChoice` calls for it (so callers get "colored on a real
/// terminal, plain when piped" just by constructing the stream with
/// `ColorChoice::Auto`). Falls back to the always-plain rendering if
/// writing to `stream` somehow fails.
fn print_diagnostic(
    stream: &mut StandardStream,
    diagnostic: &Diagnostic,
    filename: &str,
    source: &str,
) {
    if diagnostic.emit_color(filename, source, stream).is_err() {
        eprint!("{}", diagnostic.render(filename, source));
    }
}

#[derive(Parser)]
#[command(name = "mg", version, about = "Metaglyph font compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Type-check and validate source files without building.
    Check { files: Vec<PathBuf> },
    /// Build TTF instances from source files.
    Build {
        files: Vec<PathBuf>,
        #[arg(short, long)]
        out: PathBuf,
        /// `head.created`/`modified`, in seconds since the Unix epoch.
        /// Defaults to `SOURCE_DATE_EPOCH` when set, else 0, so builds
        /// are byte-identical unless asked otherwise.
        #[arg(long)]
        timestamp: Option<i64>,
    },
    /// Format source files in place.
    Fmt { files: Vec<PathBuf> },
    /// Write one glyph's outline as SVG.
    Svg {
        files: Vec<PathBuf>,
        #[arg(long)]
        glyph: String,
        #[arg(long)]
        instance: Option<String>,
        /// Show the outline as compiled: slanted, with extrema, as
        /// quadratics, zone-snapped, and rounded to integers.
        #[arg(long)]
        prepared: bool,
    },
    /// Print the evaluation dependency graph.
    DumpGraph {
        files: Vec<PathBuf>,
        #[arg(long)]
        instance: Option<String>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { files } => cmd_check(&files),
        Command::Build {
            files,
            out,
            timestamp,
        } => cmd_build(&files, &out, timestamp),
        Command::Fmt { files } => cmd_fmt(&files),
        Command::Svg {
            files,
            glyph,
            instance,
            prepared,
        } => cmd_svg(&files, &glyph, instance.as_deref(), prepared),
        Command::DumpGraph { files, instance } => cmd_dump_graph(&files, instance.as_deref()),
    }
}

fn require_files(files: &[PathBuf]) -> ExitCode {
    if files.is_empty() {
        eprintln!("error: no input files");
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

fn read_source(path: &Path) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|err| {
        eprintln!("error: could not read {}: {err}", path.display());
        ExitCode::from(2)
    })
}

fn cmd_check(files: &[PathBuf]) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    let mut had_errors = false;
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    for path in files {
        let source = match read_source(path) {
            Ok(source) => source,
            Err(code) => return code,
        };

        let parsed = mg_syntax::parse(&source);
        let filename = path.display().to_string();

        let mut file_had_errors = false;
        for diagnostic in &parsed.diagnostics {
            print_diagnostic(&mut stderr, diagnostic, &filename, &source);
            file_had_errors |= diagnostic.severity == mg_diag::Severity::Error;
        }
        had_errors |= file_had_errors;

        // A parse error's recovery can misplace whole declarations (a
        // misspelled block keyword's `{` gets skipped right along with
        // it, so what follows is lowered at the wrong scope entirely —
        // see `mg-syntax`'s recovery doc comments), which would otherwise
        // cascade into HIR diagnostics that just restate the same typo in
        // confusing, unrelated-looking ways. One file at a time, same
        // simplification M1's parser made: spec §5.6's multi-file merge
        // (every directive from every file visible everywhere) is not
        // yet implemented, so this checks each file as if it were the
        // whole font.
        if file_had_errors {
            continue;
        }
        let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let (hir, mut hir_diagnostics) = mg_hir::lower(&source_file);
        hir_diagnostics.extend(mg_font::build::check_codepoints(&hir));
        for diagnostic in &hir_diagnostics {
            print_diagnostic(&mut stderr, diagnostic, &filename, &source);
            had_errors |= diagnostic.severity == mg_diag::Severity::Error;
        }
    }

    if had_errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_build(files: &[PathBuf], out: &Path, timestamp: Option<i64>) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    // Spec §5.6's multi-file merge isn't implemented yet (see
    // `cmd_check`'s own note); the font is the first file.
    let path = &files[0];
    let source = match read_source(path) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let filename = path.display().to_string();
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    let parsed = mg_syntax::parse(&source);
    let mut had_errors = false;
    for diagnostic in &parsed.diagnostics {
        print_diagnostic(&mut stderr, diagnostic, &filename, &source);
        had_errors |= diagnostic.severity == mg_diag::Severity::Error;
    }
    if had_errors {
        return ExitCode::FAILURE;
    }

    let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax())
        .expect("SOURCE_FILE always casts from a parse's root node");
    let (hir, hir_diagnostics) = mg_hir::lower(&source_file);
    for diagnostic in &hir_diagnostics {
        print_diagnostic(&mut stderr, diagnostic, &filename, &source);
        had_errors |= diagnostic.severity == mg_diag::Severity::Error;
    }
    if had_errors {
        return ExitCode::FAILURE;
    }

    let timestamp = timestamp
        .or_else(|| std::env::var("SOURCE_DATE_EPOCH").ok()?.parse().ok())
        .unwrap_or(0);
    let options = mg_font::BuildOptions { timestamp };
    let (fonts, diagnostics) = mg_font::build_fonts(&hir, &options);
    for diagnostic in &diagnostics {
        print_diagnostic(&mut stderr, diagnostic, &filename, &source);
    }
    // `build_fonts` returns no fonts at all when anything failed (spec
    // §4.6), so a failed build writes nothing.
    if fonts.is_empty() {
        return ExitCode::FAILURE;
    }

    if let Err(err) = std::fs::create_dir_all(out) {
        eprintln!("error: could not create {}: {err}", out.display());
        return ExitCode::from(2);
    }
    for font in &fonts {
        let target = out.join(&font.file_name);
        if let Err(err) = std::fs::write(&target, &font.data) {
            eprintln!("error: could not write {}: {err}", target.display());
            return ExitCode::from(2);
        }
        eprintln!("wrote {}", target.display());
    }
    ExitCode::SUCCESS
}

fn cmd_fmt(files: &[PathBuf]) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    for path in files {
        let source = match read_source(path) {
            Ok(source) => source,
            Err(code) => return code,
        };

        let formatted = mg_syntax::fmt::format(&source);
        if formatted != source
            && let Err(err) = std::fs::write(path, &formatted)
        {
            eprintln!("error: could not write {}: {err}", path.display());
            return ExitCode::from(2);
        }
    }

    ExitCode::SUCCESS
}

fn cmd_svg(
    files: &[PathBuf],
    glyph_name: &str,
    instance_name: Option<&str>,
    prepared: bool,
) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    // Spec §5.6's multi-file merge isn't implemented yet (see `cmd_check`'s
    // own note); one glyph's outline only ever needs the first file.
    let path = &files[0];
    let source = match read_source(path) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let filename = path.display().to_string();
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    let parsed = mg_syntax::parse(&source);
    let mut had_errors = false;
    for diagnostic in &parsed.diagnostics {
        print_diagnostic(&mut stderr, diagnostic, &filename, &source);
        had_errors |= diagnostic.severity == mg_diag::Severity::Error;
    }
    if had_errors {
        return ExitCode::FAILURE;
    }

    let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax())
        .expect("SOURCE_FILE always casts from a parse's root node");
    let (hir, hir_diagnostics) = mg_hir::lower(&source_file);
    for diagnostic in &hir_diagnostics {
        print_diagnostic(&mut stderr, diagnostic, &filename, &source);
        had_errors |= diagnostic.severity == mg_diag::Severity::Error;
    }
    if had_errors {
        return ExitCode::FAILURE;
    }

    let instance = match instance_name {
        Some(name) => match hir.instances.get(name) {
            Some(instance) => instance,
            None => {
                eprintln!("error: no instance named `{name}` in {filename}");
                return ExitCode::from(2);
            }
        },
        None => hir
            .instances
            .values()
            .next()
            .expect("mg-hir always inserts at least one instance"),
    };

    if !hir.glyphs.keys().any(|(name, _)| name == glyph_name) {
        eprintln!("error: no glyph named `{glyph_name}` in {filename}");
        return ExitCode::from(2);
    }

    let (_, outcome) = mg_eval::evaluate(&hir, instance);

    if prepared {
        let (glyphs, prepare_diagnostics) = mg_font::prepare_font(&hir, instance, &outcome);
        for diagnostic in outcome.diagnostics.iter().chain(&prepare_diagnostics) {
            print_diagnostic(&mut stderr, diagnostic, &filename, &source);
        }
        let mut contours = Vec::new();
        decompose_prepared(
            &glyphs,
            glyph_name,
            kurbo::Affine::IDENTITY,
            0,
            &mut contours,
        );
        println!("{}", render_svg(&contours));
        return if outcome.diagnostics.is_empty() && prepare_diagnostics.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    let mut render_diagnostics = Vec::new();
    let contours = mg_eval::render_glyph(
        &hir,
        instance,
        glyph_name,
        &outcome.values,
        &outcome.failed,
        &mut render_diagnostics,
    );

    for diagnostic in outcome.diagnostics.iter().chain(&render_diagnostics) {
        print_diagnostic(&mut stderr, diagnostic, &filename, &source);
    }
    let Ok(contours) = contours else {
        // `render_diagnostics` already explains why; there is no partial
        // outline to fall back to without it.
        return ExitCode::FAILURE;
    };

    println!("{}", render_svg(&contours));
    // Printed regardless (this glyph rendered fine even if some unrelated
    // node elsewhere in the font failed), matching `cmd_dump_graph`'s own
    // "always show what you can, signal failure via exit code" shape.
    if outcome.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `glyph_name`'s prepared contours with every component's own placed
/// in, for previewing. Roles no longer matter by this stage, since
/// winding was fixed before slant, so every contour counts as outer for
/// [`render_svg`].
fn decompose_prepared(
    glyphs: &indexmap::IndexMap<String, mg_font::PreparedGlyph>,
    glyph_name: &str,
    transform: kurbo::Affine,
    depth: usize,
    out: &mut Vec<(kurbo::BezPath, mg_geom::winding::ContourRole)>,
) {
    let Some(glyph) = glyphs.get(glyph_name) else {
        return;
    };
    if depth >= mg_geom::tolerance::COMPONENT_DEPTH {
        return;
    }
    for contour in &glyph.contours {
        let path = transform * mg_font::outline::to_bezpath(contour);
        out.push((path, mg_geom::winding::ContourRole::Outer));
    }
    for component in &glyph.components {
        decompose_prepared(
            glyphs,
            &component.glyph,
            transform * component.transform,
            depth + 1,
            out,
        );
    }
}

/// Renders `contours` (spec §6.4–§6.5, §8: `mg_eval::render_glyph`'s
/// output, roles already resolved) as one SVG document. Every contour
/// goes into a single `<path>` with `fill-rule="nonzero"`, since that
/// rule — not this function — is what actually turns an oppositely-wound
/// counter into a hole; the roles only decided *which* direction each
/// contour got, back in `mg_eval::render_glyph`.
fn render_svg(contours: &[(kurbo::BezPath, mg_geom::winding::ContourRole)]) -> String {
    use kurbo::{Affine, Shape};

    // The glyph's own coordinates are y-up (spec §14); SVG is y-down, so
    // every contour is flipped up front rather than wrapped in a `<g>`.
    let flip = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);

    let mut bbox: Option<kurbo::Rect> = None;
    let mut data = String::new();
    for (contour, _role) in contours {
        let flipped = flip * contour.clone();
        data.push_str(&flipped.to_svg());
        data.push(' ');
        let b = flipped.bounding_box();
        bbox = Some(match bbox {
            Some(u) => u.union(b),
            None => b,
        });
    }
    let bbox = bbox.unwrap_or(kurbo::Rect::ZERO);

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\">\n\
         <path d=\"{}\" fill=\"black\" fill-rule=\"nonzero\"/>\n\
         </svg>",
        bbox.x0,
        bbox.y0,
        bbox.width(),
        bbox.height(),
        data.trim(),
    )
}

fn cmd_dump_graph(files: &[PathBuf], instance_name: Option<&str>) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    let mut had_errors = false;
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    for path in files {
        let source = match read_source(path) {
            Ok(source) => source,
            Err(code) => return code,
        };
        let filename = path.display().to_string();

        let parsed = mg_syntax::parse(&source);
        for diagnostic in &parsed.diagnostics {
            print_diagnostic(&mut stderr, diagnostic, &filename, &source);
            had_errors |= diagnostic.severity == mg_diag::Severity::Error;
        }

        let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let (hir, hir_diagnostics) = mg_hir::lower(&source_file);
        for diagnostic in &hir_diagnostics {
            print_diagnostic(&mut stderr, diagnostic, &filename, &source);
            had_errors |= diagnostic.severity == mg_diag::Severity::Error;
        }
        if had_errors {
            // Evaluating a font with unresolved names or type errors
            // would just rediscover the same problems less clearly.
            continue;
        }

        let instance = match instance_name {
            Some(name) => match hir.instances.get(name) {
                Some(instance) => instance,
                None => {
                    eprintln!("error: no instance named `{name}` in {filename}");
                    return ExitCode::from(2);
                }
            },
            None => hir
                .instances
                .values()
                .next()
                .expect("mg-hir always inserts at least one instance"),
        };

        let (graph, outcome) = mg_eval::evaluate(&hir, instance);
        let ordering = mg_eval::toposort::topo_sort(&graph);

        println!("instance {}", instance.name);
        for node in &ordering.sorted {
            let deps = graph.deps[node]
                .iter()
                .map(mg_eval::NodeId::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            match outcome.values.get(node) {
                Some(value) => println!("  {node}  <- [{deps}]  = {value}"),
                None => println!("  {node}  <- [{deps}]  = FAILED"),
            }
        }
        for node in &ordering.remaining {
            println!("  {node}  <- [cycle]  = FAILED");
        }

        for diagnostic in &outcome.diagnostics {
            print_diagnostic(&mut stderr, diagnostic, &filename, &source);
            had_errors = true;
        }
    }

    if had_errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
