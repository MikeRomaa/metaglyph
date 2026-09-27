use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
    },
    /// Print the evaluation dependency graph.
    DumpGraph { files: Vec<PathBuf> },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { files } => cmd_check(&files),
        Command::Build { files, out } => cmd_build(&files, &out),
        Command::Fmt { files } => cmd_fmt(&files),
        Command::Svg {
            files,
            glyph,
            instance,
        } => cmd_svg(&files, &glyph, instance.as_deref()),
        Command::DumpGraph { files } => cmd_dump_graph(&files),
    }
}

fn require_files(files: &[PathBuf]) -> ExitCode {
    if files.is_empty() {
        eprintln!("error: no input files");
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

fn cmd_check(files: &[PathBuf]) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    eprintln!("mg check: not yet implemented (M1 adds the lexer and parser)");
    ExitCode::FAILURE
}

fn cmd_build(files: &[PathBuf], _out: &Path) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    eprintln!("mg build: not yet implemented");
    ExitCode::FAILURE
}

fn cmd_fmt(files: &[PathBuf]) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    eprintln!("mg fmt: not yet implemented");
    ExitCode::FAILURE
}

fn cmd_svg(files: &[PathBuf], _glyph: &str, _instance: Option<&str>) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    eprintln!("mg svg: not yet implemented");
    ExitCode::FAILURE
}

fn cmd_dump_graph(files: &[PathBuf]) -> ExitCode {
    let code = require_files(files);
    if code != ExitCode::SUCCESS {
        return code;
    }

    eprintln!("mg dump-graph: not yet implemented");
    ExitCode::FAILURE
}
