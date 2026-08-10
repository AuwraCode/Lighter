//! The MCP "database": we browse the official registry
//! (registry.modelcontextprotocol.io) with server-side search + cursor
//! pagination, and normalize each entry into something installable. The
//! registry mixes camelCase and snake_case across schema versions, so every
//! field read tries both spellings and nothing panics on a missing key.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::error::{Error, Result};

const REGISTRY: &str = "https://registry.modelcontextprotocol.io/v0/servers";

/// How Lighter would install a given server. Carries the inputs (env vars /
/// headers) the user still has to fill in — `required`/`secret` drive the form.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpInstall {
    /// Remote server reached over HTTP or SSE (a URL, maybe auth headers).
    Remote {
        transport: String, // "http" | "sse"
        url: String,
        headers: Vec<McpInput>,
    },
    /// Local stdio server launched as a subprocess (npx / uvx / docker …).
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<McpInput>,
    },
    /// The registry entry had neither a usable package nor a remote.
    Unsupported { reason: String },
}

/// One value the user supplies at install time — an env var or an HTTP header.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpInput {
    /// Env var name, or header field name.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    /// Mask it in the UI and never log it.
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct McpEntry {
    /// Full registry id, e.g. `io.github.owner/repo`.
    pub name: String,
    pub display_name: String,
    /// A clean local alias suggested for `claude mcp add <alias>`.
    pub default_alias: String,
    pub description: String,
    pub version: String,
    /// Who published it, derived from the registry namespace (e.g. `gh:owner`,
    /// `smithery`). A trust-at-a-glance signal.
    pub publisher: String,
    /// Published under the official Anthropic / modelcontextprotocol namespace.
    pub official: bool,
    /// Last-updated date (YYYY-MM-DD) from the registry, if known.
    pub updated: Option<String>,
    /// Badge text: http / sse / npm / pypi / docker / ?.
    pub transport_label: String,
    pub repository: Option<String>,
    pub install: McpInstall,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CatalogPage {
    pub entries: Vec<McpEntry>,
    pub next_cursor: Option<String>,
}

/// One page of the registry, optionally filtered by `search` and continued
/// from `cursor`.
pub async fn fetch(search: Option<&str>, cursor: Option<&str>, limit: u32) -> Result<CatalogPage> {
    let mut params: Vec<(&str, String)> = vec![("limit", limit.to_string())];
    if let Some(s) = search.map(str::trim).filter(|s| !s.is_empty()) {
        params.push(("search", s.to_string()));
    }
    if let Some(c) = cursor.filter(|c| !c.is_empty()) {
        params.push(("cursor", c.to_string()));
    }
    let url = reqwest::Url::parse_with_params(REGISTRY, &params)
        .map_err(|e| Error::Control(format!("bad registry URL: {e}")))?;
    let resp = reqwest::Client::new()
        .get(url)
        .header("user-agent", "Lighter-MCP-Browser")
        .send()
        .await
        .map_err(|e| Error::Control(format!("MCP registry request failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Control(format!("MCP registry {}", resp.status())));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| Error::Control(format!("bad MCP registry JSON: {e}")))?;
    Ok(parse_page(&body))
}

fn parse_page(body: &Value) -> CatalogPage {
    let next_cursor = body
        .get("metadata")
        .and_then(|m| get_str(m, &["nextCursor", "next_cursor"]))
        .map(String::from)
        .filter(|s| !s.is_empty());
    let entries = body
        .get("servers")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(normalize_entry).collect())
        .unwrap_or_default();
    CatalogPage {
        entries,
        next_cursor,
    }
}

/// Registry entry → McpEntry. `None` only if there's no name at all.
fn normalize_entry(wrapper: &Value) -> Option<McpEntry> {
    // Entries are `{ "server": {...}, "_meta": {...} }`; tolerate a bare server.
    let s = wrapper.get("server").unwrap_or(wrapper);
    let name = get_str(s, &["name"])?.to_string();
    let description = get_str(s, &["description"]).unwrap_or("").to_string();
    let version = get_str(s, &["version"]).unwrap_or("").to_string();
    let display_name = get_str(s, &["title"])
        .map(String::from)
        .unwrap_or_else(|| short_name(&name));
    let repository = s.get("repository").and_then(|r| {
        get_str(r, &["url"])
            .map(String::from)
            .or_else(|| r.as_str().map(String::from))
    });
    // Registry status lives on the sibling `_meta`, not on `server`.
    let updated = wrapper
        .get("_meta")
        .and_then(|m| m.get("io.modelcontextprotocol.registry/official"))
        .and_then(|m| get_str(m, &["updatedAt", "publishedAt", "updated_at", "published_at"]))
        .map(|s| s.chars().take(10).collect::<String>())
        .filter(|s| !s.is_empty());
    let (install, transport_label) = derive_install(s);
    Some(McpEntry {
        default_alias: slug_alias(&name),
        publisher: publisher_of(&name),
        official: is_official(&name),
        updated,
        name,
        display_name,
        description,
        version,
        transport_label,
        repository,
        install,
    })
}

/// A friendly publisher label from the registry namespace (the part before
/// `/`): `io.github.owner` → `gh:owner`, `ai.smithery` → `smithery`.
fn publisher_of(id: &str) -> String {
    let ns = id.split('/').next().unwrap_or(id);
    if let Some(owner) = ns.strip_prefix("io.github.") {
        format!("gh:{owner}")
    } else {
        ns.rsplit('.').next().unwrap_or(ns).to_string()
    }
}

/// Published under the Anthropic / MCP official namespace.
fn is_official(id: &str) -> bool {
    let ns = id.split('/').next().unwrap_or(id);
    ns == "io.modelcontextprotocol" || ns.starts_with("com.anthropic")
}

/// Prefer a local package (fully non-interactive) over a remote; fall back to
/// a remote URL; else mark unsupported.
fn derive_install(server: &Value) -> (McpInstall, String) {
    if let Some(pkg) = get_array(server, &["packages"]).and_then(|a| a.first()) {
        return derive_stdio(pkg);
    }
    if let Some(remote) = get_array(server, &["remotes"]).and_then(|a| a.first()) {
        return derive_remote(remote);
    }
    (
        McpInstall::Unsupported {
            reason: "registry entry has no package or remote".into(),
        },
        "?".into(),
    )
}

fn derive_remote(remote: &Value) -> (McpInstall, String) {
    let raw = get_str(remote, &["type", "transportType", "transport_type"]).unwrap_or("http");
    let transport = if raw.contains("sse") { "sse" } else { "http" };
    let url = get_str(remote, &["url"]).unwrap_or("").to_string();
    let headers = inputs(remote, &["headers"]);
    (
        McpInstall::Remote {
            transport: transport.into(),
            url,
            headers,
        },
        transport.into(),
    )
}

fn derive_stdio(pkg: &Value) -> (McpInstall, String) {
    let registry_type = get_str(pkg, &["registryType", "registry_type"]).unwrap_or("");
    let identifier = get_str(pkg, &["identifier", "name"]).unwrap_or("").to_string();
    let version = get_str(pkg, &["version"]).unwrap_or("");
    let hint = get_str(pkg, &["runtimeHint", "runtime_hint"]);
    let (command, mut args) = runner_for(registry_type, hint, &identifier, version);
    append_arguments(pkg, &["runtimeArguments", "runtime_arguments"], &mut args);
    append_arguments(pkg, &["packageArguments", "package_arguments"], &mut args);
    let env = inputs(pkg, &["environmentVariables", "environment_variables"]);
    let label = match registry_type {
        "npm" => "npm",
        "pypi" => "pypi",
        "oci" | "docker" => "docker",
        "nuget" => "nuget",
        _ => "stdio",
    };
    (
        McpInstall::Stdio { command, args, env },
        label.into(),
    )
}

/// Map a package to the runner that launches it. Best-effort — the resulting
/// command is shown to the user before install.
fn runner_for(registry_type: &str, hint: Option<&str>, id: &str, version: &str) -> (String, Vec<String>) {
    let pinned = if version.is_empty() || version == "latest" {
        id.to_string()
    } else {
        format!("{id}@{version}")
    };
    match registry_type {
        "npm" => (hint.unwrap_or("npx").to_string(), vec!["-y".into(), pinned]),
        "pypi" => (hint.unwrap_or("uvx").to_string(), vec![id.to_string()]),
        "oci" | "docker" => (
            "docker".into(),
            vec!["run".into(), "-i".into(), "--rm".into(), pinned],
        ),
        _ => (hint.unwrap_or(id).to_string(), Vec::new()),
    }
}

fn append_arguments(pkg: &Value, keys: &[&str], out: &mut Vec<String>) {
    let Some(arr) = get_array(pkg, keys) else {
        return;
    };
    for a in arr {
        let atype = get_str(a, &["type"]).unwrap_or("positional");
        if atype == "named" {
            if let Some(n) = get_str(a, &["name"]) {
                out.push(n.to_string());
            }
        }
        if let Some(v) = get_str(a, &["value", "default", "valueHint", "value_hint"]) {
            out.push(v.to_string());
        }
    }
}

fn inputs(obj: &Value, keys: &[&str]) -> Vec<McpInput> {
    get_array(obj, keys)
        .map(|arr| arr.iter().filter_map(parse_input).collect())
        .unwrap_or_default()
}

fn parse_input(v: &Value) -> Option<McpInput> {
    let name = get_str(v, &["name"])?.to_string();
    Some(McpInput {
        name,
        description: get_str(v, &["description"]).unwrap_or("").to_string(),
        required: get_bool(v, &["isRequired", "is_required", "required"]),
        secret: get_bool(v, &["isSecret", "is_secret", "secret"]),
        default: get_str(v, &["default"]).map(String::from),
    })
}

// --- tolerant JSON accessors ----------------------------------------------

fn get_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()))
}

fn get_array<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_array()))
}

fn get_bool(v: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

/// The server name out of a registry id: the part after `/`
/// (`io.github.a/b` → `b`), or the last label if there's no slash.
fn short_name(id: &str) -> String {
    let after_slash = id.rsplit('/').next().unwrap_or(id);
    if after_slash == id && id.contains('.') {
        id.rsplit('.').next().unwrap_or(id).to_string()
    } else {
        after_slash.to_string()
    }
}

/// A safe local alias: short name, lowercased, non-alphanumerics → hyphens.
fn slug_alias(id: &str) -> String {
    let short = short_name(id).to_lowercase();
    let mut out = String::new();
    let mut prev_hyphen = false;
    for ch in short.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_hyphen = false;
        } else if !prev_hyphen && !out.is_empty() {
            out.push('-');
            prev_hyphen = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "mcp-server".into()
    } else {
        out
    }
}

/// GitHub popularity for a server's repo — a lightweight "rating" so users can
/// vet a server before installing. `None` on any failure (rate limit, not
/// found, non-GitHub host) so the UI simply omits it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RepoStars {
    pub stars: i64,
    pub archived: bool,
    pub url: String,
}

pub async fn fetch_repo_stars(repo_url: &str) -> Option<RepoStars> {
    let (owner, repo) = parse_github(repo_url)?;
    let api = format!("https://api.github.com/repos/{owner}/{repo}");
    let resp = reqwest::Client::new()
        .get(&api)
        .header("user-agent", "Lighter-MCP-Browser")
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    Some(RepoStars {
        stars: v.get("stargazers_count").and_then(Value::as_i64).unwrap_or(0),
        archived: v.get("archived").and_then(Value::as_bool).unwrap_or(false),
        url: v
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or(repo_url)
            .to_string(),
    })
}

/// Pull (owner, repo) from a GitHub URL, tolerating `.git`, deeper paths and a
/// trailing slash. `None` for non-GitHub URLs.
fn parse_github(url: &str) -> Option<(String, String)> {
    let rest = url.split("github.com/").nth(1)?;
    let mut parts = rest.trim_end_matches('/').split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_a_remote_server() {
        let wrapper = json!({
            "server": {
                "name": "ac.inference.sh/mcp",
                "description": "run any ai model",
                "version": "2.0.0",
                "remotes": [{ "type": "streamable-http", "url": "https://api.inference.sh/mcp" }]
            }
        });
        let e = normalize_entry(&wrapper).unwrap();
        assert_eq!(e.name, "ac.inference.sh/mcp");
        assert_eq!(e.default_alias, "mcp");
        assert_eq!(e.transport_label, "http");
        match e.install {
            McpInstall::Remote { transport, url, .. } => {
                assert_eq!(transport, "http");
                assert_eq!(url, "https://api.inference.sh/mcp");
            }
            other => panic!("expected remote, got {other:?}"),
        }
    }

    #[test]
    fn normalizes_an_npm_package_with_env() {
        let wrapper = json!({
            "server": {
                "name": "io.github.acme/weather",
                "title": "Weather",
                "version": "0.20.1",
                "packages": [{
                    "registryType": "npm",
                    "identifier": "@acme/weather-mcp",
                    "version": "0.20.1",
                    "transport": { "type": "stdio" },
                    "environmentVariables": [
                        { "name": "WEATHER_API_KEY", "isRequired": true, "isSecret": true }
                    ]
                }]
            }
        });
        let e = normalize_entry(&wrapper).unwrap();
        assert_eq!(e.display_name, "Weather");
        assert_eq!(e.transport_label, "npm");
        match e.install {
            McpInstall::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args, vec!["-y", "@acme/weather-mcp@0.20.1"]);
                assert_eq!(env.len(), 1);
                assert!(env[0].required && env[0].secret);
                assert_eq!(env[0].name, "WEATHER_API_KEY");
            }
            other => panic!("expected stdio, got {other:?}"),
        }
    }

    #[test]
    fn snake_case_variant_also_parses() {
        // Older schema spelling.
        let pkg = json!({
            "registry_type": "pypi",
            "identifier": "weather-mcp",
            "environment_variables": [{ "name": "TOKEN", "is_required": true }]
        });
        let (install, label) = derive_stdio(&pkg);
        assert_eq!(label, "pypi");
        match install {
            McpInstall::Stdio { command, env, .. } => {
                assert_eq!(command, "uvx");
                assert_eq!(env[0].name, "TOKEN");
                assert!(env[0].required);
            }
            _ => panic!("expected stdio"),
        }
    }

    #[test]
    fn page_reads_cursor_and_servers() {
        let body = json!({
            "servers": [
                { "server": { "name": "io.github.a/b", "remotes": [{ "type": "sse", "url": "https://x/y" }] } }
            ],
            "metadata": { "nextCursor": "io.github.a/b:1.0.0", "count": 1 }
        });
        let page = parse_page(&body);
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.next_cursor.as_deref(), Some("io.github.a/b:1.0.0"));
        assert_eq!(page.entries[0].transport_label, "sse");
    }

    #[test]
    fn slug_alias_is_clean() {
        assert_eq!(slug_alias("io.github.Owner/My_Cool.Server"), "my-cool-server");
        assert_eq!(slug_alias("ac.inference.sh/mcp"), "mcp");
    }

    #[test]
    fn publisher_and_official() {
        assert_eq!(publisher_of("io.github.Owner/repo"), "gh:Owner");
        assert_eq!(publisher_of("ai.smithery/foo"), "smithery");
        assert!(is_official("io.modelcontextprotocol/everything"));
        assert!(!is_official("ai.smithery/foo"));
    }

    #[test]
    fn parses_github_urls() {
        assert_eq!(
            parse_github("https://github.com/acme/weather-mcp"),
            Some(("acme".into(), "weather-mcp".into()))
        );
        assert_eq!(
            parse_github("https://github.com/acme/weather.git"),
            Some(("acme".into(), "weather".into()))
        );
        assert_eq!(
            parse_github("https://github.com/acme/weather/tree/main"),
            Some(("acme".into(), "weather".into()))
        );
        assert_eq!(parse_github("https://gitlab.com/acme/x"), None);
    }

    #[test]
    fn reads_updated_from_meta() {
        let body = json!({
            "servers": [{
                "server": { "name": "io.github.a/b", "remotes": [{ "type": "http", "url": "https://x/y" }] },
                "_meta": { "io.modelcontextprotocol.registry/official": { "updatedAt": "2026-05-01T12:00:00Z", "status": "active" } }
            }],
            "metadata": {}
        });
        let page = parse_page(&body);
        assert_eq!(page.entries[0].updated.as_deref(), Some("2026-05-01"));
        assert_eq!(page.entries[0].publisher, "gh:a");
    }
}
