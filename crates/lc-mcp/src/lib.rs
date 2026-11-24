//! Model Context Protocol (MCP) support for LeopardCraft.
//!
//! [`McpServer`] exposes a LeopardCraft document over the MCP stdio transport.
//! One newline-delimited JSON-RPC stream serves both the 2025-06-18
//! handshake revision and the 2026-07-28 handshake-free revision: classic
//! clients call `initialize`, newer clients may start with `server/discover`
//! or call tools directly.
//!
//! The server owns an in-memory document. Scene edits go through the same
//! undo commands as the editor, so `undo` and `redo` behave predictably.
//! Transport is stdio by default; [`HttpTransport`] adds the opt-in Streamable
//! HTTP transport on a loopback endpoint, and resource subscriptions and prompt
//! templates are served over both.

pub mod bridge;
mod http;
mod server;
mod tools;

pub use bridge::{BridgeJob, BridgeServer, EditorEntry, LiveClient};
pub use http::{HttpTransport, HTTP_ENDPOINT};
pub use server::{
    McpServer, DEFAULT_HTTP_ADDR, LATEST_PROTOCOL_VERSION, PROTOCOL_VERSION, SCENE_URI,
    SERVER_NAME, SUMMARY_URI,
};
