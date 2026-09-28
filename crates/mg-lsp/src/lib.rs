//! `mg lsp`: the Metaglyph language server (plan 4). It speaks JSON-RPC
//! over stdio and calls the compiler's own crates in-process, so its
//! diagnostics are exactly the compiler's.
//!
//! So far: the protocol lifecycle, a document store with full-document
//! sync, and position-encoding negotiation (L0); every diagnostic
//! `mg check` reports, document symbols, go-to-definition, and references
//! (L1); completion and static hover (L2); and evaluation off the
//! keystroke path, with per-instance diagnostics and evaluated values in
//! hover (L3).

pub mod completion;
pub mod diagnostics;
pub mod evaluation;
pub mod hover;
pub mod index;
pub mod line_index;
mod server;
pub mod symbols;
pub mod types;

pub use server::main_loop;

use lsp_server::Connection;

/// Runs the server on stdin/stdout until the client asks it to exit.
pub fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    main_loop(connection)?;
    io_threads.join()?;
    Ok(())
}
