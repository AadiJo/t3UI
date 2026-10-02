//! How work log entries read: tool identities, labels, group summaries, and the status
//! predicates behind the row indicators. Port of upstream
//! `packages/client-runtime/src/work-log/presentation.ts` (and `toolPresentation.ts`).

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::{
    text::normalize_compact_tool_label,
    work_log::{RequestKind, ToolItemType, ToolStatus, WorkLogEntry, WorkTone},
};

/// Where a tool acted (`ToolActivitySurface`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSurface {
    Browser,
    Computer,
}

/// A native app named by bundle id or display name (`ToolActivityNativeAppReference`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeApp {
    AppId(String),
    DisplayName(String),
}

/// The icon a provider attached to a tool activity (`ToolActivityIcon`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolIcon {
    Website {
        page_url: String,
        favicon_url: Option<String>,
        favicon_url_dark: Option<String>,
    },
    NativeApp(NativeApp),
    ThemedLogo {
        logo_url: String,
        logo_url_dark: Option<String>,
    },
}

/// What kind of thing a tool source is (`ToolActivitySource.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSourceKind {
    Browser,
    Computer,
    Integration,
}

/// The integration or app a tool call went through (`ToolActivitySource`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSource {
    pub key: String,
    pub name: String,
    pub kind: ToolSourceKind,
    pub icon: Option<ToolIcon>,
}

/// Surface, icon, and source read from an activity payload
/// (`toolPresentation.ts:extractToolActivityPresentation`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolActivityPresentation {
    pub surface: Option<ToolSurface>,
    pub icon: Option<ToolIcon>,
    pub source: Option<ToolSource>,
}

fn bounded(value: Option<&Value>, max: usize) -> Option<String> {
    let value = value?.as_str()?.trim();
    (!value.is_empty() && value.chars().count() <= max).then(|| value.to_owned())
}

/// `http(s)` or `data:` URLs only (`imageUrl`); `page` allows only `http(s)` (`pageUrl`).
fn url(value: Option<&Value>, page: bool) -> Option<String> {
    let url = bounded(value, 4096)?;
    let scheme = url.split_once(':')?.0.to_ascii_lowercase();
    let allowed = scheme == "http" || scheme == "https" || (!page && scheme == "data");
    allowed.then_some(url)
}

fn activity_icon(value: Option<&Value>) -> Option<ToolIcon> {
    let icon = value?.as_object()?;
    match icon.get("_tag")?.as_str()? {
        "website" => Some(ToolIcon::Website {
            page_url: url(icon.get("pageUrl"), true)?,
            favicon_url: url(icon.get("faviconUrl"), false),
            favicon_url_dark: url(icon.get("faviconUrlDark"), false),
        }),
        "native-app" => {
            let app = icon.get("app")?.as_object()?;
            match app.get("_tag")?.as_str()? {
                "app-id" => bounded(app.get("appId"), 512)
                    .filter(|id| {
                        id.chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                    })
                    .map(|id| ToolIcon::NativeApp(NativeApp::AppId(id))),
                "display-name" => bounded(app.get("displayName"), 160)
                    .map(|name| ToolIcon::NativeApp(NativeApp::DisplayName(name))),
                _ => None,
            }
        }
        "themed-logo" => Some(ToolIcon::ThemedLogo {
            logo_url: url(icon.get("logoUrl"), false)?,
            logo_url_dark: url(icon.get("logoUrlDark"), false),
        }),
        _ => None,
    }
}

/// Reads `toolSurface`, `toolIcon`, and `toolSource` from an activity payload.
pub fn extract_tool_activity_presentation(
    payload: Option<&Map<String, Value>>,
) -> ToolActivityPresentation {
    let Some(payload) = payload else {
        return ToolActivityPresentation::default();
    };
    let surface = match payload.get("toolSurface").and_then(Value::as_str) {
        Some("browser") => Some(ToolSurface::Browser),
        Some("computer") => Some(ToolSurface::Computer),
        _ => None,
    };
    let source = payload.get("toolSource").and_then(Value::as_object).and_then(|source| {
        let kind = match source.get("kind")?.as_str()? {
            "browser" => ToolSourceKind::Browser,
            "computer" => ToolSourceKind::Computer,
            "integration" => ToolSourceKind::Integration,
            _ => return None,
        };
        Some(ToolSource {
            key: bounded(source.get("key"), 512)?,
            name: bounded(source.get("name"), 160)?,
            kind,
            icon: activity_icon(source.get("icon")),
        })
    });
    ToolActivityPresentation {
        surface,
        icon: activity_icon(payload.get("toolIcon")),
        source,
    }
}

/// What a group of tool calls did, for summaries (`ToolGroupAction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolGroupAction {
    LinkPr,
    UnlinkPr,
    ListPrs,
    Read,
    Edit,
    Command,
    Browser,
    Device,
    CodeSearch,
    Search,
    Other,
    Update,
}

/// The icon family of a group summary row (`ToolGroupSummaryKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolGroupSummaryKind {
    PullRequest,
    Action(ToolGroupAction),
    DynamicTool,
    AgentTool,
    ToneTool,
    Mixed,
}

/// The glyph a T3 MCP tool call shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T3ToolIcon {
    PullRequest,
    Browser,
    Device,
    T3Code,
}

/// A recognized T3 MCP tool call: its sentence label and glyph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct T3ToolPresentation {
    pub display_name: String,
    pub icon: T3ToolIcon,
    pub action: Option<ToolGroupAction>,
}

/// `[action, running, completed, detail]` per T3 MCP tool (`T3_MCP_TOOL_LABELS`).
const T3_MCP_TOOL_LABELS: &[(&str, [&str; 4])] = &[
    ("link_pull_request", ["Link", "Linking", "Linked", "a pull request"]),
    ("unlink_pull_request", ["Unlink", "Unlinking", "Unlinked", "a pull request"]),
    ("list_thread_pull_requests", ["Check", "Checking", "Checked", "linked pull requests"]),
    ("orchestrator_capabilities", ["Get", "Getting", "Got", "orchestration capabilities"]),
    ("delegate_task", ["Delegate", "Delegating", "Delegated", "a child task"]),
    ("task_status", ["Get", "Getting", "Got", "delegated task status"]),
    ("task_cancel", ["Cancel", "Canceling", "Canceled", "delegated task"]),
    ("schedule_task", ["Schedule", "Scheduling", "Scheduled", "a recurring task"]),
    ("list_scheduled_tasks", ["List", "Listing", "Listed", "scheduled tasks"]),
    ("update_scheduled_task", ["Update", "Updating", "Updated", "a scheduled task"]),
    ("delete_scheduled_task", ["Delete", "Deleting", "Deleted", "a scheduled task"]),
    ("create_threads", ["Create", "Creating", "Created", "T3 threads"]),
    ("t3_thread_start", ["Start", "Starting", "Started", "a T3 thread"]),
    ("t3_thread_list", ["List", "Listing", "Listed", "T3 threads"]),
    ("t3_thread_read", ["Read", "Reading", "Read", "a T3 thread"]),
    ("t3_thread_send", ["Send", "Sending", "Sent", "to a T3 thread"]),
    ("t3_thread_wait", ["Wait", "Waiting", "Waited", "for a T3 thread"]),
    ("t3_thread_interrupt", ["Interrupt", "Interrupting", "Interrupted", "a T3 thread"]),
    ("t3_worktree_handoff", ["Hand off", "Handing off", "Handed off", "thread to a git worktree"]),
    ("t3_worktree_status", ["Get", "Getting", "Got", "thread worktree status"]),
    ("preview_status", ["Get", "Getting", "Got", "preview browser status"]),
    ("preview_open", ["Open", "Opening", "Opened", "a page in the preview browser"]),
    ("preview_navigate", ["Navigate", "Navigating", "Navigated", "the preview browser"]),
    (
        "preview_snapshot",
        ["Take a snapshot of", "Taking a snapshot of", "Took a snapshot of", "the preview page"],
    ),
    ("preview_click", ["Click", "Clicking", "Clicked", "in the preview browser"]),
    ("preview_press", ["Press", "Pressing", "Pressed", "a key in the preview browser"]),
    ("preview_type", ["Type", "Typing", "Typed", "in the preview browser"]),
    ("preview_scroll", ["Scroll", "Scrolling", "Scrolled", "the preview browser"]),
    ("preview_resize", ["Resize", "Resizing", "Resized", "the preview browser"]),
    ("preview_evaluate", ["Evaluate", "Evaluating", "Evaluated", "script in the preview browser"]),
    ("preview_wait_for", ["Wait", "Waiting", "Waited", "for the preview page"]),
    ("preview_set_appearance", ["Set", "Setting", "Set", "preview browser appearance"]),
    ("preview_recording_start", ["Start", "Starting", "Started", "recording the preview browser"]),
    ("preview_recording_stop", ["Stop", "Stopping", "Stopped", "recording the preview browser"]),
    ("device_list", ["List", "Listing", "Listed", "simulators and emulators"]),
    ("device_open", ["Open", "Opening", "Opened", "a device in the Device panel"]),
    (
        "device_screenshot",
        ["Take a screenshot of", "Taking a screenshot of", "Took a screenshot of", "the device"],
    ),
    ("device_close", ["Close", "Closing", "Closed", "a device"]),
];

/// Strips a `mcp__t3-code__`, `t3-code.`, `t3code:` (and similar) prefix, case-insensitively.
fn strip_t3_prefix(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for server in ["t3-code", "t3_code", "t3code"] {
        let mcp = format!("mcp__{server}__");
        if lower.starts_with(&mcp) {
            return &name[mcp.len()..];
        }
        if let Some(rest) = lower.strip_prefix(server) {
            let offset = server.len();
            if let Some(sep) = rest.chars().next().filter(|c| matches!(c, '.' | ':' | '/')) {
                return &name[offset + sep.len_utf8()..];
            }
            let trimmed = rest.trim_start();
            if let Some(after) = trimmed.strip_prefix('·') {
                let skipped = rest.len() - after.len();
                return name[offset + skipped..].trim_start();
            }
        }
    }
    name
}

/// `resolveT3McpToolPresentation`: a T3 MCP tool's sentence label for its status.
fn resolve_t3_mcp_tool(
    value: Option<&str>,
    status: Option<ToolStatus>,
    data: Option<&Value>,
) -> Option<T3ToolPresentation> {
    let normalized = normalize_compact_tool_label(value?);
    let name = strip_t3_prefix(&normalized);
    let [action, running, completed, detail] =
        T3_MCP_TOOL_LABELS.iter().find(|(key, _)| *key == name)?.1;
    let verb = match status {
        Some(ToolStatus::InProgress) | None => running.to_owned(),
        Some(ToolStatus::Completed) => completed.to_owned(),
        Some(ToolStatus::Failed) => format!("Failed to {}", action.to_lowercase()),
        Some(ToolStatus::Declined) => format!("Declined to {}", action.to_lowercase()),
        Some(ToolStatus::Stopped) => format!("Stopped {}", running.to_lowercase()),
    };
    let pr_action = match name {
        "link_pull_request" => Some(ToolGroupAction::LinkPr),
        "unlink_pull_request" => Some(ToolGroupAction::UnlinkPr),
        "list_thread_pull_requests" => Some(ToolGroupAction::ListPrs),
        _ => None,
    };
    let input = data.and_then(Value::as_object).and_then(|payload| {
        ["arguments", "input", "rawInput"]
            .iter()
            .find_map(|key| payload.get(*key)?.as_object())
    });
    let number = input.and_then(|input| {
        input
            .get("url")
            .and_then(Value::as_str)
            .and_then(pull_request_number_from_url)
            .or_else(|| input.get("number").and_then(Value::as_u64))
    });
    let target = match (pr_action, number) {
        (Some(action), Some(number)) if action != ToolGroupAction::ListPrs && number > 0 => {
            format!("PR #{number}")
        }
        _ => detail.to_owned(),
    };
    let icon = if pr_action.is_some() {
        T3ToolIcon::PullRequest
    } else if name.starts_with("preview_") {
        T3ToolIcon::Browser
    } else if name.starts_with("device_") {
        T3ToolIcon::Device
    } else {
        T3ToolIcon::T3Code
    };
    Some(T3ToolPresentation {
        display_name: format!("{verb} {target}"),
        icon,
        action: pr_action,
    })
}

/// The number in a GitHub/GitLab-style change request URL (`.../pull/12`, `.../merge_requests/12`).
fn pull_request_number_from_url(url: &str) -> Option<u64> {
    let path = url.split(['?', '#']).next()?;
    let mut segments = path.trim_end_matches('/').rsplit('/');
    let number = segments.next()?.parse().ok()?;
    matches!(segments.next()?, "pull" | "pulls" | "merge_requests" | "pull-requests")
        .then_some(number)
}

/// Latest live activity stays present tense unless the call itself failed, was declined, or
/// stopped (`liveActivityToolStatus`).
pub fn live_activity_tool_status(status: Option<ToolStatus>, present_tense: bool) -> ToolStatus {
    match status {
        Some(status @ (ToolStatus::Failed | ToolStatus::Declined | ToolStatus::Stopped)) => status,
        Some(ToolStatus::InProgress) => ToolStatus::InProgress,
        _ if present_tense => ToolStatus::InProgress,
        _ => ToolStatus::Completed,
    }
}

/// Recognizes T3's own MCP tools (`resolveWorkEntryToolPresentation`). `fallback` stands in
/// for a missing lifecycle status.
pub fn resolve_work_entry_tool_presentation(
    entry: &WorkLogEntry,
    fallback: Option<ToolStatus>,
) -> Option<T3ToolPresentation> {
    let status = entry.lifecycle_status.or(fallback);
    let data = entry.tool_data.as_ref();
    if let Some(record) = data.and_then(Value::as_object) {
        if let (Some(server), Some(tool)) = (
            record.get("server").and_then(Value::as_str),
            record.get("tool").and_then(Value::as_str),
        ) {
            return resolve_t3_mcp_tool(Some(&format!("{server}.{tool}")), status, data);
        }
        if let Some(name) = record.get("toolName").and_then(Value::as_str) {
            return resolve_t3_mcp_tool(Some(name), status, data);
        }
    }
    resolve_t3_mcp_tool(entry.tool_title.as_deref(), status, data)
        .or_else(|| resolve_t3_mcp_tool(Some(&entry.label), status, data))
}

/// Provider command output before it is formatted for a row (`extractCommandOutputText`).
pub fn extract_command_output_text(data: Option<&Value>) -> Option<String> {
    fn non_empty(value: Option<&Value>) -> Option<String> {
        let text = value?.as_str()?;
        (!text.trim().is_empty()).then(|| text.to_owned())
    }
    fn result_content(value: Option<&Value>) -> Option<String> {
        let value = value?;
        if let Some(text) = non_empty(Some(value)) {
            return Some(text);
        }
        let content = value.as_object().and_then(|record| record.get("content"));
        if let Some(text) = non_empty(content) {
            return Some(text);
        }
        let blocks = value.as_array().or_else(|| content?.as_array())?;
        let chunks: Vec<String> = blocks
            .iter()
            .filter_map(|entry| {
                non_empty(Some(entry))
                    .or_else(|| non_empty(entry.as_object().and_then(|e| e.get("text"))))
            })
            .collect();
        (!chunks.is_empty()).then(|| chunks.join("\n"))
    }
    let data = data?.as_object()?;
    let item = data.get("item").and_then(Value::as_object);
    let raw_output = data.get("rawOutput").and_then(Value::as_object);
    let streams: Vec<String> = ["stdout", "stderr"]
        .iter()
        .filter_map(|key| non_empty(raw_output.and_then(|raw| raw.get(*key))))
        .collect();
    let streams = (!streams.is_empty()).then(|| Value::String(streams.join("\n")));
    let acp = data.get("content").and_then(Value::as_array).map(|entries| {
        let texts: Vec<String> = entries
            .iter()
            .filter_map(|entry| {
                let entry = entry.as_object()?;
                if entry.get("type").and_then(Value::as_str) != Some("content") {
                    return None;
                }
                non_empty(entry.get("content")?.as_object()?.get("text"))
            })
            .collect();
        Value::String(texts.join("\n"))
    });
    let candidates = [
        item.and_then(|item| item.get("aggregatedOutput")),
        item.and_then(|item| item.get("result")?.as_object()?.get("content")),
        data.get("rawOutput"),
        raw_output.and_then(|raw| raw.get("content")),
        streams.as_ref(),
        raw_output.and_then(|raw| raw.get("output")),
        acp.as_ref(),
        data.get("result"),
    ];
    candidates.into_iter().find_map(result_content)
}

/// Whether `text` is the command itself, or a `...`-truncated prefix of it (`textRepeatsCommand`).
fn text_repeats_command(text: &str, commands: &[Option<&str>]) -> bool {
    let truncated = text
        .strip_suffix("...")
        .or_else(|| text.strip_suffix('\u{2026}'));
    commands.iter().flatten().any(|command| {
        let command = command.trim();
        !command.is_empty()
            && (command == text
                || truncated.is_some_and(|prefix| {
                    !prefix.is_empty() && command.len() > prefix.len() && command.starts_with(prefix)
                }))
    })
}

/// Whether a command row's `detail` only echoes the command (`commandDetailRepeatsCommand`).
pub fn command_detail_repeats_command(
    detail: &str,
    command: Option<&str>,
    raw_command: Option<&str>,
    tool_name: Option<&str>,
    data: Option<&Value>,
) -> bool {
    let detail = detail.trim();
    let commands = [command, raw_command];
    if let Some(tool) = tool_name.map(str::trim).filter(|t| !t.is_empty()) {
        let prefix = format!("{tool}:").to_lowercase();
        if detail.to_lowercase().starts_with(&prefix)
            && text_repeats_command(detail[prefix.len()..].trim(), &commands)
        {
            return true;
        }
    }
    if !text_repeats_command(detail, &commands) {
        return false;
    }
    let data = data.and_then(Value::as_object);
    let item = data.and_then(|d| d.get("item")).and_then(Value::as_object);
    let has_command = |value: Option<&Value>| match value {
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Array(parts)) => parts
            .iter()
            .any(|part| part.as_str().is_some_and(|p| !p.trim().is_empty())),
        _ => false,
    };
    let structured = [
        item.and_then(|i| i.get("command")),
        item.and_then(|i| i.get("input")?.as_object()?.get("command")),
        item.and_then(|i| i.get("result")?.as_object()?.get("command")),
        data.and_then(|d| d.get("command")),
    ]
    .into_iter()
    .any(has_command);
    !structured
        || item.is_some()
        || data.is_some_and(|d| d.contains_key("toolCallId"))
        || data
            .and_then(|d| d.get("kind")?.as_str())
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("execute"))
}

impl WorkLogEntry {
    /// `workLogEntryIsToolLike`: drives status indicators and grouping.
    pub fn is_tool_like(&self) -> bool {
        matches!(self.tone, WorkTone::Tool | WorkTone::Thinking | WorkTone::Error)
            || self.command.as_deref().is_some_and(|c| !c.trim().is_empty())
            || self.request_kind.is_some()
            || self.item_type.is_some()
    }

    fn failure_from_output(&self, include_command: bool) -> bool {
        if self.tone == WorkTone::Error
            || matches!(
                self.lifecycle_status,
                Some(ToolStatus::Failed | ToolStatus::Declined)
            )
        {
            return true;
        }
        if !self.is_tool_like() {
            return false;
        }
        let output = if include_command {
            [self.detail.as_deref(), self.command.as_deref()]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            self.detail.clone().unwrap_or_default()
        };
        !output.is_empty() && super::text::looks_like_failure(&output)
    }

    /// `workEntryIndicatesToolFailure`: includes legacy rows that kept error text in the command.
    pub fn indicates_failure(&self) -> bool {
        self.failure_from_output(true)
    }

    /// `workEntryDisplayIndicatesToolFailure`: checks the output without treating the command
    /// as an error. Drives the failed indicator.
    pub fn display_indicates_failure(&self) -> bool {
        self.failure_from_output(false)
    }

    /// `workEntryIndicatesToolSuccess`: the row can show a check.
    pub fn indicates_success(&self) -> bool {
        self.is_tool_like()
            && !self.indicates_failure()
            && self.tone != WorkTone::Thinking
            && !matches!(
                self.lifecycle_status,
                Some(ToolStatus::InProgress | ToolStatus::Stopped)
            )
    }

    /// `workEntryIndicatesToolNeutralStatus`: tool-like with neither success nor failure.
    /// Spawn rows are never neutral.
    pub fn indicates_neutral(&self) -> bool {
        self.agent_spawn.is_none()
            && self.is_tool_like()
            && !self.indicates_failure()
            && !self.indicates_success()
    }

    /// `workEntrySignalsSevereFailure`: the turn or a core side effect broke.
    pub fn signals_severe_failure(&self) -> bool {
        self.source_activity_kind == "runtime.error" || self.source_activity_kind.ends_with(".failed")
    }

    /// What this call did, for group summaries (`toolGroupAction`).
    pub fn tool_group_action(&self) -> ToolGroupAction {
        if matches!(
            self.source_activity_kind.as_str(),
            "approval.requested" | "approval.resolved" | "provider.approval.respond.failed"
        ) {
            return ToolGroupAction::Update;
        }
        if let Some(presentation) = resolve_work_entry_tool_presentation(self, None) {
            if let Some(action) = presentation.action {
                return action;
            }
            match presentation.icon {
                T3ToolIcon::Browser => return ToolGroupAction::Browser,
                T3ToolIcon::Device => return ToolGroupAction::Device,
                _ => {}
            }
        }
        if self.request_kind == Some(RequestKind::FileRead)
            || self.item_type == Some(ToolItemType::ImageView)
            || self.viewed_image_path.is_some()
            || (self.item_type == Some(ToolItemType::DynamicToolCall)
                && self
                    .tool_title
                    .as_deref()
                    .is_some_and(|t| t.trim().eq_ignore_ascii_case("read file")))
        {
            return ToolGroupAction::Read;
        }
        if self.request_kind == Some(RequestKind::FileChange)
            || self.item_type == Some(ToolItemType::FileChange)
            || !self.changed_files.is_empty()
        {
            return ToolGroupAction::Edit;
        }
        if self.request_kind == Some(RequestKind::Command)
            || self.item_type == Some(ToolItemType::CommandExecution)
            || self.command.is_some()
        {
            return ToolGroupAction::Command;
        }
        if self.item_type == Some(ToolItemType::WebSearch) {
            let label = normalize_compact_tool_label(self.tool_title.as_deref().unwrap_or(&self.label));
            let is_grep = label
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == "grep");
            return if is_grep {
                ToolGroupAction::CodeSearch
            } else {
                ToolGroupAction::Search
            };
        }
        if self.is_tool_like() {
            ToolGroupAction::Other
        } else {
            ToolGroupAction::Update
        }
    }

    /// A viewed workspace image this entry points at (`workEntryViewedImagePath`).
    pub fn viewed_image(&self) -> Option<&str> {
        let single_line = |p: &&str| !p.contains(['\r', '\n']) && is_workspace_image_path(p);
        if let Some(path) = self.viewed_image_path.as_deref().map(str::trim).filter(single_line) {
            return Some(path);
        }
        (self.tool_group_action() == ToolGroupAction::Read)
            .then(|| self.detail.as_deref().map(str::trim).filter(single_line))
            .flatten()
    }
}

/// Image files the file preview can show (`isWorkspaceImagePreviewPath`).
pub fn is_workspace_image_path(path: &str) -> bool {
    const EXTENSIONS: [&str; 8] = [".avif", ".gif", ".ico", ".jpeg", ".jpg", ".png", ".svg", ".webp"];
    let path = path.split(['?', '#']).next().unwrap_or("").to_lowercase();
    EXTENSIONS.iter().any(|extension| path.ends_with(extension))
}

fn action_count(action: ToolGroupAction, entries: &[&WorkLogEntry]) -> usize {
    if action != ToolGroupAction::Edit {
        return entries.len();
    }
    let mut files: HashSet<&str> = HashSet::new();
    let mut without_files = 0;
    for entry in entries {
        if entry.changed_files.is_empty() {
            without_files += 1;
        }
        files.extend(entry.changed_files.iter().map(String::as_str));
    }
    files.len() + without_files
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn action_label(action: ToolGroupAction, count: usize) -> String {
    match action {
        ToolGroupAction::LinkPr => format!("Linked {}", plural(count, "pull request", "pull requests")),
        ToolGroupAction::UnlinkPr => {
            format!("Unlinked {}", plural(count, "pull request", "pull requests"))
        }
        ToolGroupAction::ListPrs if count == 1 => "Checked linked pull requests".to_owned(),
        ToolGroupAction::ListPrs => format!("Checked linked pull requests {count} times"),
        ToolGroupAction::Read => format!("Read {}", plural(count, "file", "files")),
        ToolGroupAction::Edit => format!("Changed {}", plural(count, "file", "files")),
        ToolGroupAction::Command => format!("Ran {}", plural(count, "command", "commands")),
        ToolGroupAction::Device => format!("Used device controls {}", plural(count, "time", "times")),
        ToolGroupAction::Browser => format!("Used browser {}", plural(count, "time", "times")),
        ToolGroupAction::Search => format!("Searched the web {}", plural(count, "time", "times")),
        ToolGroupAction::CodeSearch => format!("Searched code {}", plural(count, "time", "times")),
        ToolGroupAction::Other => format!("Used {}", plural(count, "tool", "tools")),
        ToolGroupAction::Update => format!("Received {}", plural(count, "update", "updates")),
    }
}

/// Joins `a`, `a and b`, `a, b, and c`.
fn join_sentence(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// One sentence for a group of calls: "Ran 3 commands and read 2 files" (`summarizeToolGroup`).
pub fn summarize_tool_group(entries: &[&WorkLogEntry]) -> String {
    let entries = omit_superseded_lifecycle_markers(entries, |entry| entry);
    let mut sources: Vec<&ToolSource> = Vec::new();
    let mut groups: Vec<(ToolGroupAction, Vec<&WorkLogEntry>)> = Vec::new();
    for entry in entries {
        if let Some(source) = &entry.tool_source
            && resolve_work_entry_tool_presentation(entry, None)
                .is_none_or(|p| p.icon != T3ToolIcon::PullRequest)
        {
            match sources.iter_mut().find(|known| known.key == source.key) {
                Some(known) => *known = source,
                None => sources.push(source),
            }
            continue;
        }
        let action = entry.tool_group_action();
        match groups.iter_mut().find(|(known, _)| *known == action) {
            Some((_, group)) => group.push(entry),
            None => groups.push((action, vec![entry])),
        }
    }
    let mut labels: Vec<String> = groups
        .iter()
        .map(|(action, group)| action_label(*action, action_count(*action, group)))
        .collect();
    if !sources.is_empty() {
        let names: Vec<String> = sources.iter().map(|s| s.name.clone()).collect();
        let all_integrations = sources.iter().all(|s| s.kind == ToolSourceKind::Integration);
        let suffix = match (all_integrations, sources.len()) {
            (false, _) => "",
            (true, 1) => " integration",
            (true, _) => " integrations",
        };
        labels.insert(0, format!("Used {}{suffix}", join_sentence(&names)));
    }
    let labels: Vec<String> = labels
        .into_iter()
        .enumerate()
        .map(|(index, label)| if index == 0 { label } else { lowercase_first(&label) })
        .collect();
    join_sentence(&labels)
}

fn lowercase_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Drops status-less, id-less `tool.started`/`tool.updated` markers that a later terminal
/// entry of the same tool supersedes (`omitSupersededLifecycleMarkers`).
pub fn omit_superseded_lifecycle_markers<'a, T>(
    entries: &[T],
    entry_for: impl Fn(&T) -> &WorkLogEntry,
) -> Vec<T>
where
    T: Clone + 'a,
{
    let mut terminal: HashSet<String> = HashSet::new();
    let mut kept: Vec<T> = Vec::with_capacity(entries.len());
    for item in entries.iter().rev() {
        let entry = entry_for(item);
        let identity = [
            entry.turn_id.as_ref().map_or("no-turn", |t| t.as_str()),
            entry.item_type.map_or("", ToolItemType::as_str),
            &normalize_compact_tool_label(entry.tool_title.as_deref().unwrap_or(&entry.label)),
        ]
        .join("\u{1f}");
        let kind = entry.source_activity_kind.as_str();
        let marker = entry.tool_call_id.is_none()
            && entry.lifecycle_status.is_none()
            && (kind == "tool.started" || kind == "tool.updated");
        if marker && terminal.contains(&identity) {
            continue;
        }
        kept.push(item.clone());
        if kind == "tool.completed"
            || entry
                .lifecycle_status
                .is_some_and(|status| status != ToolStatus::InProgress)
        {
            terminal.insert(identity);
        }
    }
    kept.reverse();
    kept
}

/// The icon family of a group summary (`toolGroupSummaryKind`).
pub fn tool_group_summary_kind(entries: &[&WorkLogEntry]) -> ToolGroupSummaryKind {
    if !entries.is_empty()
        && entries.iter().all(|entry| {
            resolve_work_entry_tool_presentation(entry, None)
                .is_some_and(|p| p.icon == T3ToolIcon::PullRequest)
        })
    {
        return ToolGroupSummaryKind::PullRequest;
    }
    let actions: HashSet<ToolGroupAction> = entries.iter().map(|e| e.tool_group_action()).collect();
    if actions.len() != 1 {
        return ToolGroupSummaryKind::Mixed;
    }
    let action = *actions.iter().next().expect("one action");
    if action != ToolGroupAction::Other {
        return ToolGroupSummaryKind::Action(action);
    }
    let kinds: HashSet<ToolGroupSummaryKind> = entries
        .iter()
        .map(|entry| match entry.item_type {
            Some(ToolItemType::McpToolCall) => ToolGroupSummaryKind::Action(ToolGroupAction::Other),
            Some(ToolItemType::DynamicToolCall) => ToolGroupSummaryKind::DynamicTool,
            Some(ToolItemType::CollabAgentToolCall) => ToolGroupSummaryKind::AgentTool,
            _ if entry.task_id.is_some() || entry.tone == WorkTone::Thinking => {
                ToolGroupSummaryKind::AgentTool
            }
            _ if entry.tone == WorkTone::Tool => ToolGroupSummaryKind::ToneTool,
            _ => ToolGroupSummaryKind::Action(ToolGroupAction::Other),
        })
        .collect();
    if kinds.len() == 1 {
        *kinds.iter().next().expect("one kind")
    } else {
        ToolGroupSummaryKind::Mixed
    }
}

/// The program a shell command runs, for "Running npm" labels. A simplified port of
/// `commandLabel.ts:commandProgramName` (the web parses full shell syntax): skips
/// environment assignments and common wrappers (`sudo`, `env`, `time`, `command`, `exec`,
/// `nohup`, `cd x &&`), then returns the first word's basename.
pub fn command_program_name(command: &str) -> Option<String> {
    let mut rest = command.trim();
    // `cd dir && npm test` names `npm`.
    loop {
        let lower = rest.to_ascii_lowercase();
        let is_cd = lower.starts_with("cd ") || lower.starts_with("pushd ");
        match (is_cd, rest.find("&&")) {
            (true, Some(index)) => rest = rest[index + 2..].trim_start(),
            _ => break,
        }
    }
    const WRAPPERS: [&str; 8] = ["sudo", "env", "time", "command", "exec", "nohup", "builtin", "noglob"];
    for token in rest.split_whitespace() {
        let token = token.trim_matches(|c| c == '"' || c == '\'');
        if token.is_empty() || token.starts_with('-') {
            continue;
        }
        if let Some((name, _)) = token.split_once('=')
            && !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        if WRAPPERS.contains(&token) {
            continue;
        }
        if token.starts_with(['<', '>', '(', ')', '{', '}', '[', ']', ';', '|', '&', '$', '`', '#'])
        {
            return None;
        }
        let name = token.rsplit(['/', '\\']).next().unwrap_or(token);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    //! Failure modes: T3 MCP prefixes not stripped (so tools read as raw ids); PR numbers
    //! read from the wrong URL segment; group summaries counting edits per call instead of per
    //! file, missing the sentence joiners, or capitalizing later clauses; superseded markers
    //! kept (double rows) or terminal rows dropped; program names returning a wrapper (`sudo`)
    //! or an assignment.
    use super::*;
    use serde_json::json;

    fn entry(label: &str) -> WorkLogEntry {
        WorkLogEntry::for_test(label)
    }

    #[test]
    fn t3_tools_read_as_sentences() {
        let mut call = entry("t3-code · link_pull_request");
        call.lifecycle_status = Some(ToolStatus::Completed);
        call.tool_data = Some(json!({"arguments": {"url": "https://github.com/a/b/pull/42"}}));
        let presentation = resolve_work_entry_tool_presentation(&call, None).unwrap();
        assert_eq!(presentation.display_name, "Linked PR #42");
        assert_eq!(presentation.icon, T3ToolIcon::PullRequest);
        assert_eq!(call.tool_group_action(), ToolGroupAction::LinkPr);

        let mut preview = entry("mcp__t3-code__preview_click");
        preview.lifecycle_status = Some(ToolStatus::Failed);
        assert_eq!(
            resolve_work_entry_tool_presentation(&preview, None)
                .unwrap()
                .display_name,
            "Failed to click in the preview browser"
        );
        assert_eq!(preview.tool_group_action(), ToolGroupAction::Browser);
        assert!(resolve_work_entry_tool_presentation(&entry("Ran command"), None).is_none());
    }

    #[test]
    fn group_summaries() {
        let mut ls = entry("Ran command");
        ls.command = Some("ls".into());
        ls.item_type = Some(ToolItemType::CommandExecution);
        let mut edit = entry("File change");
        edit.item_type = Some(ToolItemType::FileChange);
        edit.changed_files = vec!["a.ts".into(), "b.ts".into()];
        let mut edit_again = edit.clone();
        edit_again.changed_files = vec!["a.ts".into()];
        let mut search = entry("Web search");
        search.item_type = Some(ToolItemType::WebSearch);
        assert_eq!(summarize_tool_group(&[&ls, &ls]), "Ran 2 commands");
        assert_eq!(
            summarize_tool_group(&[&ls, &edit, &edit_again, &search]),
            "Ran 1 command, changed 2 files, and searched the web 1 time"
        );
        assert_eq!(
            tool_group_summary_kind(&[&ls, &ls]),
            ToolGroupSummaryKind::Action(ToolGroupAction::Command)
        );
        assert_eq!(tool_group_summary_kind(&[&ls, &edit]), ToolGroupSummaryKind::Mixed);
    }

    #[test]
    fn superseded_markers_are_dropped() {
        let mut started = entry("Read");
        started.source_activity_kind = "tool.updated".into();
        let mut done = entry("Read");
        done.source_activity_kind = "tool.completed".into();
        done.lifecycle_status = Some(ToolStatus::Completed);
        let kept = omit_superseded_lifecycle_markers(&[&started, &done], |e| e);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].source_activity_kind, "tool.completed");
    }

    #[test]
    fn program_names() {
        assert_eq!(command_program_name("npm test").as_deref(), Some("npm"));
        assert_eq!(command_program_name("sudo -E env FOO=1 /usr/bin/node x.js").as_deref(), Some("node"));
        assert_eq!(command_program_name("cd app && pnpm i").as_deref(), Some("pnpm"));
        assert_eq!(command_program_name("FOO=bar").as_deref(), None);
    }

    #[test]
    fn command_output_and_echo_detection() {
        let data = json!({"item": {"command": "ls", "aggregatedOutput": "a\nb"}});
        assert_eq!(extract_command_output_text(Some(&data)).as_deref(), Some("a\nb"));
        assert!(command_detail_repeats_command("ls", Some("ls"), None, None, Some(&data)));
        assert!(!command_detail_repeats_command("total 2", Some("ls"), None, None, Some(&data)));
    }

    #[test]
    fn tool_activity_presentation() {
        let payload = json!({
            "toolSurface": "browser",
            "toolSource": {"key": "gh", "name": "GitHub", "kind": "integration",
                           "icon": {"_tag": "website", "pageUrl": "https://github.com"}},
            "toolIcon": {"_tag": "native-app", "app": {"_tag": "app-id", "appId": "bad id"}}
        });
        let presentation = extract_tool_activity_presentation(payload.as_object());
        assert_eq!(presentation.surface, Some(ToolSurface::Browser));
        assert_eq!(presentation.icon, None);
        assert_eq!(presentation.source.unwrap().name, "GitHub");
    }
}
