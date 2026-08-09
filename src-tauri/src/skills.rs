//! Auto-provisioning of Claude Code skill plugins from the anthropics/skills
//! marketplace. Plugins install at `user` scope into a config dir, so once
//! ensured they apply to every session/repo launched from that account.
//!
//! Verified: print-mode (`-p`) sessions DO surface these — they show up in the
//! init frame as `example-skills:frontend-design`, `document-skills:xlsx`, etc.

use std::process::Command;

use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::error::{Error, Result};

pub const MARKETPLACE_SOURCE: &str = "anthropics/skills";
pub const MARKETPLACE_NAME: &str = "anthropic-agent-skills";

/// The plugins Lighter can auto-install, with what each bundles (for the UI).
pub const AVAILABLE: &[(&str, &str)] = &[
    (
        "example-skills",
        "skill-creator, webapp-testing, mcp-builder, frontend-design",
    ),
    ("document-skills", "docx, pdf, pptx, xlsx"),
];

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SkillPluginInfo {
    pub id: String,
    pub bundles: String,
    pub installed: bool,
}

fn claude_cmd(config_dir: Option<&str>) -> Result<Command> {
    let bin = crate::session::spawn::resolve_claude_bin()?;
    let mut cmd = Command::new(bin);
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    Ok(cmd)
}

fn run(config_dir: Option<&str>, args: &[&str]) -> Result<String> {
    let output = claude_cmd(config_dir)?
        .args(args)
        .output()
        .map_err(|e| Error::Control(format!("claude {}: {e}", args.join(" "))))?;
    if !output.status.success() {
        return Err(Error::Control(format!(
            "claude {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Plugin ids installed in this config dir (e.g. "example-skills@anthropic-agent-skills").
pub fn installed_ids(config_dir: Option<&str>) -> Vec<String> {
    let Ok(out) = run(config_dir, &["plugin", "list", "--json"]) else {
        return Vec::new();
    };
    serde_json::from_str::<Value>(&out)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn is_installed(installed: &[String], plugin: &str) -> bool {
    let qualified = format!("{plugin}@{MARKETPLACE_NAME}");
    installed.iter().any(|id| id == &qualified)
}

pub fn info(config_dir: Option<&str>) -> Vec<SkillPluginInfo> {
    let installed = installed_ids(config_dir);
    AVAILABLE
        .iter()
        .map(|(id, bundles)| SkillPluginInfo {
            id: id.to_string(),
            bundles: bundles.to_string(),
            installed: is_installed(&installed, id),
        })
        .collect()
}

/// Idempotently ensure the requested plugins are installed at user scope.
/// Registers the marketplace on first use. Returns Ok even if some installs
/// fail — the error is logged; a session should never be blocked by this.
pub fn ensure(config_dir: Option<&str>, plugins: &[String]) -> Result<()> {
    if plugins.is_empty() {
        return Ok(());
    }
    let valid: Vec<&String> = plugins
        .iter()
        .filter(|p| AVAILABLE.iter().any(|(id, _)| *id == p.as_str()))
        .collect();
    let installed = installed_ids(config_dir);
    let missing: Vec<&String> = valid
        .into_iter()
        .filter(|p| !is_installed(&installed, p))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    // Register the marketplace (idempotent: "already on disk" is success).
    if let Err(e) = run(config_dir, &["plugin", "marketplace", "add", MARKETPLACE_SOURCE]) {
        tracing::warn!(%e, "failed to add skills marketplace");
        return Err(e);
    }

    for plugin in missing {
        let qualified = format!("{plugin}@{MARKETPLACE_NAME}");
        match run(config_dir, &["plugin", "install", &qualified, "--scope", "user"]) {
            Ok(_) => tracing::info!(plugin = %qualified, ?config_dir, "installed skill plugin"),
            Err(e) => tracing::warn!(%e, plugin = %qualified, "failed to install skill plugin"),
        }
    }
    Ok(())
}
