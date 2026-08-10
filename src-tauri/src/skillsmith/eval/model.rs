//! Model access for the eval, auto-detected: the Anthropic Messages API when
//! ANTHROPIC_API_KEY is set (temperature 0, forced tool-use — deterministic and
//! fully controls the system prompt), otherwise `claude -p` on the user's
//! subscription with a clean overriding system prompt (less controllable, but
//! no API key required). Either way the model sees ONLY our catalog prompt.

use serde_json::{json, Value};

use crate::error::{Error, Result};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const API_MODEL: &str = "claude-haiku-4-5-20251001";
const CLI_MODEL: &str = "haiku";
const MAX_TOKENS: u32 = 512;

#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub skill: Option<String>,
    pub also_plausible: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Model {
    Api { key: String },
    Cli { config_dir: Option<String> },
}

impl Model {
    /// API when a key is present, else the CLI on subscription auth.
    pub fn detect(config_dir: Option<String>) -> Model {
        match std::env::var("ANTHROPIC_API_KEY") {
            Ok(k) if !k.trim().is_empty() => Model::Api { key: k },
            _ => Model::Cli { config_dir },
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Model::Api { .. } => "api",
            Model::Cli { .. } => "cli",
        }
    }

    /// Route one query against the catalog. `system` is the routing prompt.
    pub async fn route(&self, system: &str, query: &str) -> Result<RouteDecision> {
        let value = match self {
            Model::Api { key } => api_tool_call(key, system, query).await?,
            Model::Cli { config_dir } => {
                // No forced tool on the CLI, so frame the query as DATA to
                // classify — otherwise the model just answers it.
                let prompt = format!(
                    "{system}\n\n---\nDecide routing for the query below. Do NOT answer the query; \
only classify which skill (if any) you would consult.\n\nQuery:\n{query}\n\n\
Return ONLY JSON: {{\"skill\": \"<name or null>\", \"also_plausible\": [\"<name>\"]}}"
                );
                cli_complete(config_dir.as_deref(), &prompt).await?
            }
        };
        Ok(RouteDecision {
            skill: value
                .get("skill")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && *s != "null" && *s != "none")
                .map(String::from),
            also_plausible: value
                .get("also_plausible")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        })
    }

    /// General structured completion: the model must return a JSON object.
    pub async fn complete_json(&self, system: &str, user: &str) -> Result<Value> {
        match self {
            Model::Api { key } => api_json(key, system, user).await,
            Model::Cli { config_dir } => {
                let prompt = format!(
                    "{system}\n\n{user}\n\nReturn ONLY a JSON object, no prose, no code fences."
                );
                cli_complete(config_dir.as_deref(), &prompt).await
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Anthropic Messages API

async fn api_tool_call(key: &str, system: &str, query: &str) -> Result<Value> {
    let body = json!({
        "model": API_MODEL,
        "max_tokens": MAX_TOKENS,
        "temperature": 0,
        "system": system,
        "messages": [{ "role": "user", "content": query }],
        "tools": [{
            "name": "route",
            "description": "Report the routing decision for this query.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "skill": { "type": ["string", "null"], "description": "chosen skill name, or null" },
                    "also_plausible": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["skill"]
            }
        }],
        "tool_choice": { "type": "tool", "name": "route" }
    });
    let resp = post(key, &body).await?;
    // content[] → find tool_use named "route"
    resp["content"]
        .as_array()
        .and_then(|blocks| {
            blocks
                .iter()
                .find(|b| b["type"] == "tool_use" && b["name"] == "route")
                .map(|b| b["input"].clone())
        })
        .ok_or_else(|| Error::Control("model returned no route tool_use".into()))
}

async fn api_json(key: &str, system: &str, user: &str) -> Result<Value> {
    let sys = format!("{system}\n\nRespond with ONLY a JSON object, no prose, no code fences.");
    let body = json!({
        "model": API_MODEL,
        "max_tokens": 1024,
        "temperature": 0,
        "system": sys,
        "messages": [{ "role": "user", "content": user }]
    });
    let resp = post(key, &body).await?;
    let text = resp["content"][0]["text"].as_str().unwrap_or_default();
    extract_json(text).ok_or_else(|| Error::Control(format!("non-JSON model reply: {text}")))
}

async fn post(key: &str, body: &Value) -> Result<Value> {
    let client = reqwest::Client::new();
    let resp = client
        .post(API_URL)
        .header("x-api-key", key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| Error::Control(format!("API request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::Control(format!("API {status}: {text}")));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| Error::Control(format!("bad API JSON: {e}")))
}

// ---------------------------------------------------------------------------
// claude -p (subscription auth). Everything — routing rules, catalog, query —
// goes in the prompt as data to classify. --safe-mode keeps the user's own
// installed skills out of the environment so they can't bias the router.

async fn cli_complete(config_dir: Option<&str>, prompt: &str) -> Result<Value> {
    let bin = crate::session::spawn::resolve_claude_bin()?;
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args([
        "-p",
        prompt,
        "--model",
        CLI_MODEL,
        "--output-format",
        "json",
        "--safe-mode",
    ]);
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = cmd
        .output()
        .await
        .map_err(|e| Error::Control(format!("claude -p failed: {e}")))?;
    if !output.status.success() {
        return Err(Error::Control(format!(
            "claude -p exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| Error::Control(format!("claude -p non-JSON envelope: {e}")))?;
    let result = envelope["result"].as_str().unwrap_or_default();
    extract_json(result).ok_or_else(|| Error::Control(format!("model reply not JSON: {result}")))
}

/// Pull the first balanced {...} JSON object out of a possibly-noisy reply.
fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim().trim_start_matches("```json").trim_start_matches("```").trim();
    if let Ok(v) = serde_json::from_str::<Value>(t.trim_end_matches("```").trim()) {
        return Some(v);
    }
    let start = t.find('{')?;
    let mut depth = 0i32;
    for (i, ch) in t[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&t[start..start + i + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::extract_json;

    #[test]
    fn extract_json_handles_fences_and_prose() {
        assert_eq!(
            extract_json("```json\n{\"skill\": \"pdf\"}\n```").unwrap()["skill"],
            "pdf"
        );
        assert_eq!(
            extract_json("Sure! {\"skill\": null, \"also_plausible\": []} done").unwrap()["skill"],
            serde_json::Value::Null
        );
    }
}
