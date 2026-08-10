//! Browse the official MCP registry and install servers into an account
//! through the `claude mcp` CLI. `catalog` is the read side (search the
//! registry, normalize entries); `manage` is the write side (add/remove/list).

pub mod catalog;
pub mod manage;

pub use catalog::{fetch, CatalogPage, McpEntry, McpInput, McpInstall};
pub use manage::{build_add_args, InstalledMcp};
