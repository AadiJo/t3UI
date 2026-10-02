//! Open approvals and user-input questions of a thread, derived from its activities. Port of
//! upstream `packages/client-runtime/src/pendingRequests.ts` (`derivePendingRequests`,
//! protocol.md 5.6). Answer with `commands::respond_to_approval` / `respond_to_user_input` /
//! `dismiss_user_input`.

use std::{collections::HashSet, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use t3_protocol::{
    ApprovalRequestId, open_enum,
    orchestration::{ApprovalDecision, OrchestrationThreadActivity},
};

open_enum! {
    /// What an approval is for.
    pub enum RequestKind {
        Command = "command",
        FileRead = "file-read",
        FileChange = "file-change",
        McpElicitation = "mcp-elicitation",
        Permission = "permission",
    }
}

/// An approval waiting for the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub request_id: ApprovalRequestId,
    pub request_kind: RequestKind,
    pub created_at: String,
    pub detail: Option<String>,
    pub app_name: Option<String>,
    /// Provider-specific choices; empty means the default accept/decline set.
    pub options: Vec<ApprovalOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalOption {
    pub decision: ApprovalDecision,
    pub label: String,
    /// Caution shown next to the option (e.g. a prompt injection warning).
    pub warning: Option<String>,
}

/// Questions waiting for answers. Answer keys are question ids; values are the chosen option
/// label (or labels for `multi_select`), or free text when custom answers are allowed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUserInput {
    pub request_id: ApprovalRequestId,
    pub created_at: String,
    pub questions: Vec<UserInputQuestion>,
    /// Async questions (`responseMode: "message"`) can be dismissed without a reply.
    pub dismissible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<UserInputOption>,
    pub allow_custom_answer: Option<bool>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputOption {
    pub label: String,
    pub description: String,
    pub value: Option<String>,
}

/// Everything a thread is waiting on, each list sorted by `created_at`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRequests {
    pub approvals: Vec<PendingApproval>,
    pub user_inputs: Vec<PendingUserInput>,
}

impl PendingRequests {
    pub fn is_empty(&self) -> bool {
        self.approvals.is_empty() && self.user_inputs.is_empty()
    }
}

/// Older activities carry a native request type instead of a request kind.
fn kind_from_request_type(request_type: Option<&str>) -> Option<RequestKind> {
    Some(match request_type? {
        "command_execution_approval" | "exec_command_approval" | "dynamic_tool_call" => {
            RequestKind::Command
        }
        "file_read_approval" => RequestKind::FileRead,
        "file_change_approval" | "apply_patch_approval" => RequestKind::FileChange,
        "mcp_elicitation_approval" => RequestKind::McpElicitation,
        "permission_approval" => RequestKind::Permission,
        _ => return None,
    })
}

/// The server reports a stale or unknown request through the failure text. A failed reply with
/// any other text stays open so the user can retry.
fn is_stale_failure(kind: &str, payload: &Map<String, Value>) -> bool {
    let fragments: &[&str] = match kind {
        "provider.approval.respond.failed" => &[
            "stale pending approval request",
            "unknown pending approval request",
            "unknown pending permission request",
            "unknown pending codex approval request",
        ],
        "provider.user-input.respond.failed" => &[
            "stale pending user-input request",
            "unknown pending user-input request",
            "unknown pending user input request",
            "unknown pending codex user input request",
        ],
        _ => return false,
    };
    let detail = payload
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    fragments.iter().any(|fragment| detail.contains(fragment))
}

/// Questions with at least one valid option (or that allow a custom answer). Ids and labels are
/// answer keys, so they are not trimmed.
fn parse_questions(value: Option<&Value>) -> Vec<UserInputQuestion> {
    let Some(Value::Array(questions)) = value else {
        return Vec::new();
    };
    questions
        .iter()
        .filter_map(|question| {
            let question = question.as_object()?;
            let options: Vec<UserInputOption> = question
                .get("options")?
                .as_array()?
                .iter()
                .filter_map(|option| serde_json::from_value(option.clone()).ok())
                .collect();
            let allow_custom_answer = question.get("allowCustomAnswer").and_then(Value::as_bool);
            if options.is_empty() && allow_custom_answer == Some(false) {
                return None;
            }
            let text = |key: &str| question.get(key)?.as_str().map(str::to_owned);
            Some(UserInputQuestion {
                id: text("id")?,
                header: text("header")?,
                question: text("question")?,
                options,
                allow_custom_answer,
                multi_select: question.get("multiSelect").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

/// Reduces a thread's activities to its open requests (upstream `derivePendingRequests`). A
/// terminal activity closes a request regardless of the order activities arrive in.
pub fn pending_requests(activities: &[Arc<OrchestrationThreadActivity>]) -> PendingRequests {
    let mut approvals: Vec<PendingApproval> = Vec::new();
    let mut user_inputs: Vec<PendingUserInput> = Vec::new();
    let mut closed_approvals: HashSet<ApprovalRequestId> = HashSet::new();
    let mut closed_user_inputs: HashSet<ApprovalRequestId> = HashSet::new();

    for activity in activities {
        let kind = activity.kind.as_str();
        if !matches!(
            kind,
            "approval.requested"
                | "approval.resolved"
                | "provider.approval.respond.failed"
                | "user-input.requested"
                | "user-input.resolved"
                | "provider.user-input.respond.failed"
        ) {
            continue;
        }
        let Some(payload) = activity.payload.as_object() else {
            continue;
        };
        let Some(request_id) = payload
            .get("requestId")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(ApprovalRequestId::from)
        else {
            continue;
        };
        let text = |key: &str| {
            payload
                .get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };

        match kind {
            "approval.requested" => {
                let request_type = payload.get("requestType").and_then(Value::as_str);
                if closed_approvals.contains(&request_id)
                    || matches!(
                        request_type,
                        Some("tool_user_input" | "auth_tokens_refresh")
                    )
                {
                    continue;
                }
                let request_kind = payload
                    .get("requestKind")
                    .and_then(Value::as_str)
                    .map(RequestKind::from)
                    .filter(|kind| !matches!(kind, RequestKind::Other(_)))
                    .or_else(|| kind_from_request_type(request_type))
                    // Older OpenCode approvals do not always include a recognized kind.
                    .unwrap_or(RequestKind::Command);
                let options = payload
                    .get("options")
                    .and_then(Value::as_array)
                    .map(|options| {
                        options
                            .iter()
                            .filter_map(|option| serde_json::from_value(option.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                approvals.retain(|a| a.request_id != request_id);
                approvals.push(PendingApproval {
                    request_id,
                    request_kind,
                    created_at: activity.created_at.clone(),
                    detail: text("detail"),
                    app_name: text("appName"),
                    options,
                });
            }
            "user-input.requested" => {
                if closed_user_inputs.contains(&request_id) {
                    continue;
                }
                let questions = parse_questions(payload.get("questions"));
                if questions.is_empty() {
                    continue;
                }
                user_inputs.retain(|u| u.request_id != request_id);
                user_inputs.push(PendingUserInput {
                    request_id,
                    created_at: activity.created_at.clone(),
                    questions,
                    dismissible: payload.get("responseMode").and_then(Value::as_str)
                        == Some("message"),
                });
            }
            "approval.resolved" | "provider.approval.respond.failed"
                if kind == "approval.resolved" || is_stale_failure(kind, payload) =>
            {
                approvals.retain(|a| a.request_id != request_id);
                closed_approvals.insert(request_id);
            }
            "user-input.resolved" | "provider.user-input.respond.failed"
                if kind == "user-input.resolved" || is_stale_failure(kind, payload) =>
            {
                user_inputs.retain(|u| u.request_id != request_id);
                closed_user_inputs.insert(request_id);
            }
            _ => {}
        }
    }

    approvals.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    user_inputs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    PendingRequests {
        approvals,
        user_inputs,
    }
}
