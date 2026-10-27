//! Polishing through Claude Code or Codex: the pure logic of docs/spec/polishing.md. Processes live
//! in the `shepit-agent` crate; nothing here spawns or does I/O.

pub mod claude_wire;
pub mod codex_wire;
pub mod discovery;
pub mod lifecycle;
pub mod pipeline;
pub mod prompt;
pub mod reply;
pub mod route;
