//! Frames Lighter writes to the CLI's stdin (newline-delimited JSON).
//!
//! Shapes verified against claude 2.1.226 by the probe fixtures
//! (`tests/fixtures/*.ndjson`, `to_cli` lines).

use serde::Serialize;
use serde_json::{json, Value};

/// A user turn. Slash commands (e.g. "/compact") are sent as plain text.
pub fn user_message(text: &str) -> Value {
    json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{ "type": "text", "text": text }],
        },
    })
}

/// Control requests Lighter can issue to the CLI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlRequest {
    Initialize {
        hooks: Option<Value>,
    },
    Interrupt,
    SetPermissionMode {
        mode: String,
    },
    SetModel {
        model: String,
    },
}

pub fn control_request(request_id: &str, request: &ControlRequest) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": request,
    })
}

/// The decision payload for answering a `can_use_tool` control request.
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    /// Allow this call. `updated_input` echoes (or rewrites) the tool input.
    /// `updated_permissions` carries `permission_suggestions` entries back to
    /// persist an "always allow" rule.
    Allow {
        updated_input: Value,
        updated_permissions: Option<Value>,
    },
    Deny {
        message: String,
        interrupt: bool,
    },
}

pub fn permission_response(request_id: &str, decision: &PermissionDecision) -> Value {
    let response = match decision {
        PermissionDecision::Allow {
            updated_input,
            updated_permissions,
        } => {
            let mut r = json!({ "behavior": "allow", "updatedInput": updated_input });
            if let Some(perms) = updated_permissions {
                r["updatedPermissions"] = perms.clone();
            }
            r
        }
        PermissionDecision::Deny { message, interrupt } => {
            json!({ "behavior": "deny", "message": message, "interrupt": interrupt })
        }
    };
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": response,
        },
    })
}

/// Error reply to a control request we cannot handle (unknown subtype etc.).
pub fn control_error_response(request_id: &str, error: &str) -> Value {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "error",
            "request_id": request_id,
            "error": error,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_shape() {
        let v = user_message("hello");
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["content"][0]["text"], "hello");
    }

    #[test]
    fn control_request_shapes() {
        let v = control_request(
            "req_1",
            &ControlRequest::SetPermissionMode { mode: "plan".into() },
        );
        assert_eq!(v["request"]["subtype"], "set_permission_mode");
        assert_eq!(v["request"]["mode"], "plan");

        let v = control_request("req_2", &ControlRequest::Interrupt);
        assert_eq!(v["request"]["subtype"], "interrupt");
    }

    #[test]
    fn permission_response_shapes() {
        let allow = permission_response(
            "id1",
            &PermissionDecision::Allow {
                updated_input: json!({"command": "ls"}),
                updated_permissions: None,
            },
        );
        assert_eq!(allow["response"]["response"]["behavior"], "allow");
        assert!(allow["response"]["response"]
            .get("updatedPermissions")
            .is_none());

        let deny = permission_response(
            "id2",
            &PermissionDecision::Deny {
                message: "no".into(),
                interrupt: true,
            },
        );
        assert_eq!(deny["response"]["response"]["behavior"], "deny");
        assert_eq!(deny["response"]["response"]["interrupt"], true);
    }
}
