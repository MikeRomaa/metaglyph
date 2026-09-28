//! `mg lsp`: the Metaglyph language server (plan 4). It speaks JSON-RPC
//! over stdio and calls the compiler's own crates in-process, so its
//! diagnostics are exactly the compiler's.
//!
//! L0 (this much): the protocol lifecycle, a document store with
//! full-document sync, position-encoding negotiation, and syntax
//! diagnostics on every change.

pub mod diagnostics;
pub mod line_index;
mod server;

pub use server::main_loop;

use lsp_server::Connection;

/// Runs the server on stdin/stdout until the client asks it to exit.
pub fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    main_loop(connection)?;
    io_threads.join()?;
    Ok(())
}
