//! Installing / removing / listing MCP servers through the `claude mcp` CLI,
//! scoped to an account via CLAUDE_CONFIG_DIR (like the rest of Lighter).
//! `local`/`project` scopes are per-repo, so those need a project dir as the
//! working directory; `user` scope is global.

use std::collections::HashMap;
use std::process::Command;

use serde::Serialize;
use ts_rs::TS;

use crate::error::{Error, Result};

use super::catalog::{McpInput, McpInstall};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct InstalledMcp {
    pub name: String,
    /// The command or URL as `claude mcp list` prints it.
    pub detail: String,
    /// connected | failed | pending | unknown.
    pub status: String,
}

/// Resolve an input's value: an explicit user value wins, else its default.
/// Errors if a required input is left empty; returns None to skip an optional
/// one that wasn't filled.
fn value_for(input: &McpInput, values: &HashMap<String, String>) -> Result<Option<String>> {
    let v = values
        .get(&input.name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| input.default.clone());
    match v {
        Some(v) => Ok(Some(v)),
        None if input.required => Err(Error::InvalidInput(format!(
            "missing required value: {}",
            input.name
        ))),
        None => Ok(None),
    }
}

/// Build the `claude mcp add …` argument vector for an install. Kept pure so it
/// can be unit-tested without spawning anything.
pub fn build_add_args(
    alias: &str,
    install: &McpInstall,
    scope: &str,
    values: &HashMap<String, String>,
) -> Result<Vec<String>> {
    if alias.trim().is_empty() {
        return Err(Error::InvalidInput("a server name is required".into()));
    }
    match install {
        McpInstall::Remote {
            transport,
            url,
            headers,
        } => {
            if url.trim().is_empty() {
                return Err(Error::InvalidInput("remote server has no URL".into()));
            }
            let mut a = vec![
                "mcp".into(),
                "add".into(),
                "--transport".into(),
                transport.clone(),
                alias.into(),
                url.clone(),
                "-s".into(),
                scope.into(),
            ];
            for h in headers {
                if let Some(val) = value_for(h, values)? {
                    a.push("-H".into());
                    a.push(format!("{}: {}", h.name, val));
                }
            }
            Ok(a)
        }
        McpInstall::Stdio { command, args, env } => {
            if command.trim().is_empty() {
                return Err(Error::InvalidInput("server has no launch command".into()));
            }
            let mut a = vec!["mcp".into(), "add".into(), alias.into(), "-s".into(), scope.into()];
            for e in env {
                if let Some(val) = value_for(e, values)? {
                    a.push("-e".into());
                    a.push(format!("{}={}", e.name, val));
                }
            }
            a.push("--".into());
            a.push(command.clone());
            a.extend(args.clone());
            Ok(a)
        }
        McpInstall::Unsupported { reason } => {
            Err(Error::InvalidInput(format!("cannot install: {reason}")))
        }
    }
}

fn claude(config_dir: Option<&str>, project_dir: Option<&str>) -> Result<Command> {
    let bin = crate::session::spawn::resolve_claude_bin()?;
    let mut cmd = Command::new(bin);
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    if let Some(dir) = project_dir.filter(|d| !d.trim().is_empty()) {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    Ok(cmd)
}

fn run(config_dir: Option<&str>, project_dir: Option<&str>, args: &[String]) -> Result<String> {
    let output = claude(config_dir, project_dir)?
        .args(args)
        .output()
        .map_err(|e| Error::Control(format!("claude mcp failed: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        return Err(Error::Control(if msg.is_empty() {
            stdout.trim().to_string()
        } else {
            msg.to_string()
        }));
    }
    Ok(stdout.trim().to_string())
}

pub fn install(
    config_dir: Option<&str>,
    project_dir: Option<&str>,
    args: &[String],
) -> Result<String> {
    let out = run(config_dir, project_dir, args)?;
    Ok(if out.is_empty() {
        "Added.".into()
    } else {
        out
    })
}

pub fn remove(
    config_dir: Option<&str>,
    project_dir: Option<&str>,
    scope: Option<&str>,
    name: &str,
) -> Result<String> {
    let mut args = vec!["mcp".into(), "remove".into(), name.to_string()];
    if let Some(s) = scope.filter(|s| !s.is_empty()) {
        args.push("-s".into());
        args.push(s.to_string());
    }
    let out = run(config_dir, project_dir, &args)?;
    Ok(if out.is_empty() {
        "Removed.".into()
    } else {
        out
    })
}

/// Parse `claude mcp list` (no --json flag exists) into structured rows.
pub fn list_installed(config_dir: Option<&str>, project_dir: Option<&str>) -> Vec<InstalledMcp> {
    let Ok(out) = run(config_dir, project_dir, &["mcp".into(), "list".into()]) else {
        return Vec::new();
    };
    out.lines().filter_map(parse_list_line).collect()
}

/// A server row looks like `name: <command-or-url> - ✓ Connected`. Header and
/// blank lines (no `:`) are skipped.
fn parse_list_line(line: &str) -> Option<InstalledMcp> {
    let line = line.trim();
    let (name, rest) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() || name.contains(' ') {
        return None; // prose like "Checking MCP server health:" — not a row.
    }
    let lower = rest.to_lowercase();
    let status = if lower.contains("connect") || rest.contains('✓') {
        "connected"
    } else if lower.contains("fail") || lower.contains("error") || rest.contains('✗') {
        "failed"
    } else if lower.contains("pending") || rest.contains('⏸') {
        "pending"
    } else {
        "unknown"
    };
    // Strip the trailing " - <status>" so `detail` is just the command/URL.
    let detail = rest
        .rsplit_once(" - ")
        .map(|(d, _)| d)
        .unwrap_or(rest)
        .trim()
        .to_string();
    Some(InstalledMcp {
        name: name.to_string(),
        detail,
        status: status.to_string(),
    })
}

/// Open a console running `claude mcp login <name>` for OAuth/browser flows,
/// mirroring the account sign-in terminal.
pub fn open_login_terminal(config_dir: Option<&str>, name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::InvalidInput("server name is required".into()));
    }
    let mut cmd = Command::new("cmd.exe");
    let set_part = config_dir
        .filter(|d| !d.trim().is_empty())
        .map(|d| format!("set CLAUDE_CONFIG_DIR={d}&& "))
        .unwrap_or_default();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(format!(
            "/c start \"MCP sign-in\" cmd /k \"{set_part}claude mcp login {name}\""
        ));
    }
    cmd.spawn()
        .map_err(|e| Error::Control(format!("failed to open MCP sign-in terminal: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::catalog::McpInput;

    fn input(name: &str, required: bool) -> McpInput {
        McpInput {
            name: name.into(),
            description: String::new(),
            required,
            secret: false,
            default: None,
        }
    }

    #[test]
    fn remote_add_args_with_header() {
        let install = McpInstall::Remote {
            transport: "http".into(),
            url: "https://mcp.example.com/mcp".into(),
            headers: vec![input("Authorization", true)],
        };
        let mut values = HashMap::new();
        values.insert("Authorization".into(), "Bearer abc".into());
        let args = build_add_args("sentry", &install, "user", &values).unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--transport",
                "http",
                "sentry",
                "https://mcp.example.com/mcp",
                "-s",
                "user",
                "-H",
                "Authorization: Bearer abc",
            ]
        );
    }

    #[test]
    fn stdio_add_args_with_env() {
        let install = McpInstall::Stdio {
            command: "npx".into(),
            args: vec!["-y".into(), "@acme/weather-mcp@1.0.0".into()],
            env: vec![input("WEATHER_API_KEY", true)],
        };
        let mut values = HashMap::new();
        values.insert("WEATHER_API_KEY".into(), "k-123".into());
        let args = build_add_args("weather", &install, "project", &values).unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "weather",
                "-s",
                "project",
                "-e",
                "WEATHER_API_KEY=k-123",
                "--",
                "npx",
                "-y",
                "@acme/weather-mcp@1.0.0",
            ]
        );
    }

    #[test]
    fn missing_required_value_errors() {
        let install = McpInstall::Stdio {
            command: "npx".into(),
            args: vec![],
            env: vec![input("TOKEN", true)],
        };
        assert!(build_add_args("x", &install, "user", &HashMap::new()).is_err());
    }

    #[test]
    fn optional_empty_value_is_skipped() {
        let install = McpInstall::Stdio {
            command: "npx".into(),
            args: vec!["srv".into()],
            env: vec![input("OPTIONAL", false)],
        };
        let args = build_add_args("x", &install, "user", &HashMap::new()).unwrap();
        assert!(!args.iter().any(|a| a == "-e"));
    }

    #[test]
    fn parses_list_rows() {
        let connected = parse_list_line("sentry: https://mcp.sentry.dev/mcp (HTTP) - ✓ Connected").unwrap();
        assert_eq!(connected.name, "sentry");
        assert_eq!(connected.status, "connected");
        assert_eq!(connected.detail, "https://mcp.sentry.dev/mcp (HTTP)");

        let pending = parse_list_line("acme: npx -y acme - ⏸ Pending approval").unwrap();
        assert_eq!(pending.status, "pending");

        // Header / prose lines are ignored.
        assert!(parse_list_line("Checking MCP server health:").is_none());
        assert!(parse_list_line("").is_none());
    }
}
