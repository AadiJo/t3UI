//! Work log entries derived from thread activities (`session-logic.ts`
//! `deriveWorkLogEntries`, `stabilizeWorkLogEntryPresentation` and the tool status predicates).

use std::{cmp::Ordering, collections::HashMap, sync::Arc};

use serde_json::{Map, Value};
use t3_protocol::{
    TurnId,
    orchestration::{ActivityTone, OrchestrationThreadActivity},
};

use super::text::{
    encode_uri_component, format_command_array_part, looks_like_failure,
    normalize_compact_tool_label, normalize_inline_preview, preview_for_comparison,
    strip_trailing_exit_code, trimmed, truncate_inline_preview, unwrap_shell_wrapper,
};

/// How a work entry is colored and which fallback icon it gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkTone {
    /// `task.progress` (the reasoning presentation).
    Thinking,
    Tool,
    /// Info and approval activities. Unknown tones from a newer server land here too.
    Info,
    Error,
}

/// Provider item types that count as tool calls (`TOOL_LIFECYCLE_ITEM_TYPES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolItemType {
    CommandExecution,
    FileChange,
    McpToolCall,
    DynamicToolCall,
    CollabAgentToolCall,
    WebSearch,
    ImageView,
}

impl ToolItemType {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "command_execution" => Self::CommandExecution,
            "file_change" => Self::FileChange,
            "mcp_tool_call" => Self::McpToolCall,
            "dynamic_tool_call" => Self::DynamicToolCall,
            "collab_agent_tool_call" => Self::CollabAgentToolCall,
            "web_search" => Self::WebSearch,
            "image_view" => Self::ImageView,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::CommandExecution => "command_execution",
            Self::FileChange => "file_change",
            Self::McpToolCall => "mcp_tool_call",
            Self::DynamicToolCall => "dynamic_tool_call",
            Self::CollabAgentToolCall => "collab_agent_tool_call",
            Self::WebSearch => "web_search",
            Self::ImageView => "image_view",
        }
    }
}

/// What an approval request asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestKind {
    Command,
    FileRead,
    FileChange,
}

impl RequestKind {
    /// `payload.requestKind`, else the canonical `payload.requestType`.
    fn from_payload(payload: &Map<String, Value>) -> Option<Self> {
        match payload.get("requestKind").and_then(Value::as_str) {
            Some("command") => return Some(Self::Command),
            Some("file-read") => return Some(Self::FileRead),
            Some("file-change") => return Some(Self::FileChange),
            _ => {}
        }
        match payload.get("requestType").and_then(Value::as_str)? {
            "command_execution_approval" | "exec_command_approval" | "dynamic_tool_call" => {
                Some(Self::Command)
            }
            "file_read_approval" => Some(Self::FileRead),
            "file_change_approval" | "apply_patch_approval" => Some(Self::FileChange),
            _ => None,
        }
    }
}

/// `payload.status` of tool lifecycle activities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
    Stopped,
}

impl ToolStatus {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "inProgress" => Self::InProgress,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "declined" => Self::Declined,
            "stopped" => Self::Stopped,
            _ => return None,
        })
    }
}

/// One work log row: a tool call, command, file change, web search, MCP call, approval or
/// user-input request, plan update, or reasoning step.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkLogEntry {
    /// The activity id (of the first lifecycle event when updates were collapsed).
    pub id: String,
    /// Provider identity shared by every lifecycle update of one tool call.
    pub tool_call_id: Option<String>,
    /// The identity this client first presented the entry under (see [`WorkLogPresenter`]).
    pub presentation_id: Option<String>,
    pub created_at: String,
    pub turn_id: Option<TurnId>,
    pub label: String,
    pub detail: Option<String>,
    /// Display command, shell wrapper removed.
    pub command: Option<String>,
    /// The original command when it differs from [`Self::command`].
    pub raw_command: Option<String>,
    /// Up to 12 paths found in the payload.
    pub changed_files: Vec<String>,
    pub tone: WorkTone,
    pub tool_title: Option<String>,
    /// The MCP call item, shown as JSON in the expanded body.
    pub tool_data: Option<Value>,
    pub item_type: Option<ToolItemType>,
    pub request_kind: Option<RequestKind>,
    pub lifecycle_status: Option<ToolStatus>,
    /// The activity kind (`tool.completed`, `user-input.requested`, ...).
    pub source_activity_kind: String,
}

impl WorkLogEntry {
    /// The entry's timeline identity (`workLogEntryPresentationId`).
    pub fn presentation_key(&self) -> String {
        if let Some(id) = &self.presentation_id {
            return id.clone();
        }
        match &self.tool_call_id {
            Some(call) => format!("tool:{call}"),
            None => self.id.clone(),
        }
    }

    fn has_command(&self) -> bool {
        self.command
            .as_deref()
            .is_some_and(|c| !c.trim().is_empty())
    }

    /// `workLogEntryIsToolLike`: drives status indicators and the "Work Log" label.
    pub fn is_tool_like(&self) -> bool {
        matches!(
            self.tone,
            WorkTone::Tool | WorkTone::Thinking | WorkTone::Error
        ) || self.has_command()
            || self.request_kind.is_some()
            || self.item_type.is_some()
    }

    /// `workLogEntryIsToolCall`: actual tool calls, excluding thinking and runtime errors.
    /// Groups made only of these render as a tool-call stack.
    pub fn is_tool_call(&self) -> bool {
        match self.source_activity_kind.as_str() {
            "task.progress" | "task.completed" => return false,
            "tool.updated" | "tool.completed" => return true,
            _ => {}
        }
        self.tone == WorkTone::Tool
            || self.has_command()
            || self.request_kind.is_some()
            || self.item_type.is_some()
    }

    /// `workEntryIndicatesToolFailure`: explicit status or tone, or error-shaped output.
    pub fn indicates_failure(&self) -> bool {
        if self.tone == WorkTone::Error {
            return true;
        }
        if matches!(
            self.lifecycle_status,
            Some(ToolStatus::Failed | ToolStatus::Declined)
        ) {
            return true;
        }
        if !self.is_tool_like() {
            return false;
        }
        let blob = [self.detail.as_deref(), self.command.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        !blob.is_empty() && looks_like_failure(&blob)
    }

    /// `workEntryIndicatesToolSuccess`: a finished tool row without failure.
    pub fn indicates_success(&self) -> bool {
        self.is_tool_like()
            && !self.indicates_failure()
            && self.tone != WorkTone::Thinking
            && !matches!(
                self.lifecycle_status,
                Some(ToolStatus::InProgress | ToolStatus::Stopped)
            )
    }

    /// `workEntryIndicatesToolNeutralStatus`: tool-like with neither success nor failure
    /// (in progress, stopped, thinking).
    pub fn indicates_neutral(&self) -> bool {
        self.is_tool_like() && !self.indicates_failure() && !self.indicates_success()
    }

    /// The row heading: tool title (or label) without a completion suffix, first letter
    /// capitalized (`toolWorkEntryHeading`).
    pub fn heading(&self) -> String {
        let source = self.tool_title.as_deref().unwrap_or(&self.label);
        capitalize(&normalize_compact_tool_label(source))
    }
}

fn capitalize(value: &str) -> String {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => value.to_owned(),
    }
}

/// Activity order (`compareActivitiesByOrder`): provider sequence when both have one (entries
/// without a sequence first), then `created_at`, then lifecycle rank, then id.
pub fn compare_activities(
    left: &OrchestrationThreadActivity,
    right: &OrchestrationThreadActivity,
) -> Ordering {
    match (left.sequence, right.sequence) {
        (Some(a), Some(b)) if a != b => return a.cmp(&b),
        (Some(_), None) => return Ordering::Greater,
        (None, Some(_)) => return Ordering::Less,
        _ => {}
    }
    left.created_at
        .cmp(&right.created_at)
        .then_with(|| lifecycle_rank(&left.kind).cmp(&lifecycle_rank(&right.kind)))
        .then_with(|| left.id.cmp(&right.id))
}

fn lifecycle_rank(kind: &str) -> u8 {
    if kind.ends_with(".started") {
        0
    } else if kind.ends_with(".completed") || kind.ends_with(".resolved") {
        2
    } else {
        1
    }
}

/// An entry plus the bookkeeping only collapse needs.
struct Derived {
    entry: WorkLogEntry,
    collapse_key: Option<String>,
    legacy_key: Option<String>,
}

fn is_lifecycle(kind: &str) -> bool {
    kind == "tool.updated" || kind == "tool.completed"
}

/// Turns activities into work log entries: drops lifecycle noise, maps each activity, and
/// collapses consecutive updates of one tool call into a single entry.
pub fn derive_work_log_entries(
    activities: &[Arc<OrchestrationThreadActivity>],
) -> Vec<WorkLogEntry> {
    let mut ordered: Vec<&OrchestrationThreadActivity> =
        activities.iter().map(Arc::as_ref).collect();
    ordered.sort_by(|a, b| compare_activities(a, b));

    let mut collapsed: Vec<Derived> = Vec::new();
    for activity in ordered {
        if matches!(
            activity.kind.as_str(),
            "tool.started" | "task.started" | "context-window.updated"
        ) || activity.summary == "Checkpoint captured"
            || is_plan_boundary(activity)
        {
            continue;
        }
        let next = derive_entry(activity);
        match collapsed.last_mut() {
            Some(previous) if should_collapse(previous, &next) => merge(previous, next),
            _ => collapsed.push(next),
        }
    }

    let mut occurrences: HashMap<String, usize> = HashMap::new();
    collapsed
        .into_iter()
        .map(|derived| {
            let mut entry = derived.entry;
            if let Some(key) = derived.legacy_key {
                let occurrence = occurrences.entry(key.clone()).or_default();
                entry.presentation_id = Some(format!("{key}:{occurrence}"));
                *occurrence += 1;
            }
            entry
        })
        .collect()
}

/// `ExitPlanMode:` lifecycle rows are replaced by the plan card.
fn is_plan_boundary(activity: &OrchestrationThreadActivity) -> bool {
    is_lifecycle(&activity.kind)
        && activity
            .payload
            .get("detail")
            .and_then(Value::as_str)
            .is_some_and(|detail| detail.starts_with("ExitPlanMode:"))
}

fn non_empty_str<'a>(map: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a str> {
    map?.get(key)?.as_str().filter(|value| !value.is_empty())
}

fn trimmed_str<'a>(map: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a str> {
    trimmed(map?.get(key)?.as_str()?)
}

fn object<'a>(map: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a Map<String, Value>> {
    map?.get(key)?.as_object()
}

fn derive_entry(activity: &OrchestrationThreadActivity) -> Derived {
    let payload = activity.payload.as_object();
    let (command, raw_command) = extract_command(payload);
    let changed_files = extract_changed_files(payload);
    let title = trimmed_str(payload, "title").map(str::to_owned);
    let is_task = matches!(activity.kind.as_str(), "task.progress" | "task.completed");
    let task_summary = non_empty_str(payload, "summary").filter(|_| is_task);
    let task_detail_label =
        non_empty_str(payload, "detail").filter(|_| is_task && task_summary.is_none());
    let detail = if is_task {
        non_empty_str(payload, "detail")
            .filter(|_| task_detail_label.is_none())
            .and_then(|detail| strip_trailing_exit_code(detail).0)
    } else {
        extract_detail(payload, title.as_deref().unwrap_or(&activity.summary))
    };
    let item_type = payload
        .and_then(|p| p.get("itemType"))
        .and_then(Value::as_str)
        .and_then(ToolItemType::parse);
    let tool_data = (item_type == Some(ToolItemType::McpToolCall))
        .then(|| {
            object(payload, "data")
                .and_then(|data| data.get("item"))
                .cloned()
        })
        .flatten();
    let lifecycle_status = payload
        .and_then(|p| p.get("status"))
        .and_then(Value::as_str)
        .and_then(ToolStatus::parse)
        .or((activity.kind == "tool.completed").then_some(ToolStatus::Completed));
    let tone = if activity.kind == "task.progress" {
        WorkTone::Thinking
    } else {
        match activity.tone {
            ActivityTone::Tool => WorkTone::Tool,
            ActivityTone::Error => WorkTone::Error,
            ActivityTone::Info | ActivityTone::Approval | ActivityTone::Other(_) => WorkTone::Info,
        }
    };
    let entry = WorkLogEntry {
        id: activity.id.to_string(),
        // Upstream reads `payload.toolCallId` first (nightly puts it there); the fork only
        // reads `payload.data.toolCallId`.
        tool_call_id: (!is_task)
            .then(|| {
                trimmed_str(payload, "toolCallId")
                    .or_else(|| trimmed_str(object(payload, "data"), "toolCallId"))
                    .map(str::to_owned)
            })
            .flatten(),
        presentation_id: None,
        created_at: activity.created_at.clone(),
        turn_id: activity.turn_id.clone(),
        label: task_summary
            .or(task_detail_label)
            .unwrap_or(&activity.summary)
            .to_owned(),
        detail,
        command,
        raw_command,
        changed_files,
        tone,
        tool_title: title,
        tool_data,
        item_type,
        request_kind: payload.and_then(RequestKind::from_payload),
        lifecycle_status,
        source_activity_kind: activity.kind.clone(),
    };
    let collapse_key = collapse_key(&entry);
    let legacy_key = legacy_presentation_key(&entry, collapse_key.as_deref());
    Derived {
        entry,
        collapse_key,
        legacy_key,
    }
}

fn collapse_key(entry: &WorkLogEntry) -> Option<String> {
    if !is_lifecycle(&entry.source_activity_kind) {
        return None;
    }
    if let Some(call) = &entry.tool_call_id {
        return Some(format!("tool:{call}"));
    }
    let label = normalize_compact_tool_label(entry.tool_title.as_deref().unwrap_or(&entry.label));
    let detail = entry.detail.as_deref().map(str::trim).unwrap_or("");
    let item_type = entry.item_type.map_or("", ToolItemType::as_str);
    if label.is_empty() && detail.is_empty() && item_type.is_empty() {
        return None;
    }
    Some([item_type, &label, detail].join("\u{1f}"))
}

/// Presentation key for lifecycle rows without a tool-call id; numbered per occurrence later.
fn legacy_presentation_key(entry: &WorkLogEntry, collapse_key: Option<&str>) -> Option<String> {
    if entry.tool_call_id.is_some() || !is_lifecycle(&entry.source_activity_kind) {
        return None;
    }
    let turn = entry.turn_id.as_ref().map_or("", |turn| turn.as_str());
    let lifecycle = match collapse_key {
        Some(key) => key.to_owned(),
        None => [
            entry.item_type.map_or("", ToolItemType::as_str),
            &normalize_compact_tool_label(entry.tool_title.as_deref().unwrap_or(&entry.label)),
        ]
        .join("\u{1f}"),
    };
    Some(format!(
        "legacy-tool:{}:{}",
        encode_uri_component(turn),
        encode_uri_component(&lifecycle)
    ))
}

fn should_collapse(previous: &Derived, next: &Derived) -> bool {
    let (prev, nxt) = (&previous.entry, &next.entry);
    if !is_lifecycle(&prev.source_activity_kind)
        || !is_lifecycle(&nxt.source_activity_kind)
        || prev.source_activity_kind == "tool.completed"
    {
        return false;
    }
    if previous.collapse_key.is_some() && previous.collapse_key == next.collapse_key {
        return true;
    }
    prev.tool_call_id.is_none() != nxt.tool_call_id.is_none()
        && prev.item_type == nxt.item_type
        && normalize_compact_tool_label(prev.tool_title.as_deref().unwrap_or(&prev.label))
            == normalize_compact_tool_label(nxt.tool_title.as_deref().unwrap_or(&nxt.label))
}

/// Lifecycle updates describe one visible call: take the newer fields, but keep the first
/// activity's id and position so a completion never remounts or reorders the row.
fn merge(previous: &mut Derived, next: Derived) {
    let Derived {
        entry: mut merged,
        collapse_key,
        legacy_key,
    } = next;
    let prev = &mut previous.entry;
    merged.id = std::mem::take(&mut prev.id);
    merged.created_at = std::mem::take(&mut prev.created_at);
    merged.detail = merged.detail.or(prev.detail.take());
    merged.command = merged.command.or(prev.command.take());
    merged.raw_command = merged.raw_command.or(prev.raw_command.take());
    let mut files = std::mem::take(&mut prev.changed_files);
    for file in merged.changed_files.drain(..) {
        if !files.contains(&file) {
            files.push(file);
        }
    }
    merged.changed_files = files;
    merged.tool_title = merged.tool_title.or(prev.tool_title.take());
    merged.item_type = merged.item_type.or(prev.item_type);
    merged.request_kind = merged.request_kind.or(prev.request_kind);
    merged.tool_call_id = merged.tool_call_id.or(prev.tool_call_id.take());
    merged.lifecycle_status = merged.lifecycle_status.or(prev.lifecycle_status);
    merged.tool_data = merged.tool_data.or(prev.tool_data.take());
    previous.collapse_key = collapse_key.or(previous.collapse_key.take());
    previous.legacy_key = previous.legacy_key.take().or(legacy_key);
    previous.entry = merged;
}

/// `formatCommandValue`: a trimmed string, or argv parts joined with quoting.
fn format_command_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return trimmed(text).map(str::to_owned);
    }
    let parts: Vec<String> = value
        .as_array()?
        .iter()
        .filter_map(|part| trimmed(part.as_str()?))
        .map(format_command_array_part)
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// `extractToolCommand`: the first command among the item, its input and result, the data, or
/// (for command executions) the detail. Returns the display command and the raw one when the
/// shell wrapper was removed.
fn extract_command(payload: Option<&Map<String, Value>>) -> (Option<String>, Option<String>) {
    let data = object(payload, "data");
    let item = object(data, "item");
    let detail_command = (trimmed_str(payload, "itemType") == Some("command_execution"))
        .then(|| trimmed_str(payload, "detail"))
        .flatten()
        .and_then(|detail| strip_trailing_exit_code(detail).0)
        .map(Value::String);
    let candidates = [
        item.and_then(|item| item.get("command")),
        object(item, "input").and_then(|input| input.get("command")),
        object(item, "result").and_then(|result| result.get("command")),
        data.and_then(|data| data.get("command")),
        detail_command.as_ref(),
    ];
    for candidate in candidates.into_iter().flatten() {
        let Some(formatted) = format_command_value(candidate) else {
            continue;
        };
        let command = unwrap_shell_wrapper(&formatted);
        let raw = (formatted != command).then_some(formatted);
        return (Some(command), raw);
    }
    (None, None)
}

/// `extractToolDetail`: the detail unless it only repeats the heading; for non-command tools a
/// raw-output summary instead.
fn extract_detail(payload: Option<&Map<String, Value>>, heading: &str) -> Option<String> {
    let detail = trimmed_str(payload, "detail").and_then(|d| strip_trailing_exit_code(d).0);
    let heading_key = preview_for_comparison(Some(heading));
    if let Some(detail) = &detail
        && preview_for_comparison(Some(detail)) != heading_key
    {
        return Some(detail.clone());
    }
    if is_command_detail(payload, heading) {
        return None;
    }
    let summary = summarize_raw_output(payload)?;
    (preview_for_comparison(Some(&summary)) != heading_key).then_some(summary)
}

fn is_command_detail(payload: Option<&Map<String, Value>>, heading: &str) -> bool {
    let item_type_is_command = payload
        .and_then(|p| p.get("itemType"))
        .and_then(Value::as_str)
        == Some("command_execution");
    let kind = trimmed_str(object(payload, "data"), "kind").map(str::to_lowercase);
    // `payload.title ?? heading`: a non-string title (null) falls back to the heading.
    let title = match payload.and_then(|p| p.get("title")) {
        Some(Value::String(title)) => trimmed(title),
        _ => trimmed(heading),
    }
    .map(str::to_lowercase);
    item_type_is_command
        || kind.as_deref() == Some("execute")
        || matches!(title.as_deref(), Some("terminal" | "ran command"))
}

fn summarize_raw_output(payload: Option<&Map<String, Value>>) -> Option<String> {
    let raw = object(object(payload, "data"), "rawOutput")?;
    if let Some(total) = raw
        .get("totalFiles")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
    {
        let suffix = if raw.get("truncated") == Some(&Value::Bool(true)) {
            "+"
        } else {
            ""
        };
        let plural = if total == 1.0 { "" } else { "s" };
        return Some(format!("{} file{plural}{suffix}", format_count(total)));
    }
    trimmed_str(Some(raw), "content")
        .or_else(|| trimmed_str(Some(raw), "stdout"))
        .and_then(summarize_text_output)
}

/// First meaningful output line, or "N lines" (`summarizeToolTextOutput`).
fn summarize_text_output(value: &str) -> Option<String> {
    let lines: Vec<String> = value
        .split('\n')
        .map(|line| normalize_inline_preview(line.strip_suffix('\r').unwrap_or(line)))
        .filter(|line| !line.is_empty())
        .collect();
    if let Some(first) = lines.iter().find(|line| *line != "```") {
        return Some(truncate_inline_preview(first));
    }
    (lines.len() > 1).then(|| format!("{} lines", format_count(lines.len() as f64)))
}

/// `Number.prototype.toLocaleString()` in en-US for integers: `1234` → `1,234`.
fn format_count(value: f64) -> String {
    if value.fract() != 0.0 || value.abs() >= 1e15 {
        return value.to_string();
    }
    let digits = (value.abs() as u64).to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if value < 0.0 { format!("-{out}") } else { out }
}

const MAX_CHANGED_FILES: usize = 12;

/// Up to 12 distinct paths from the payload data, searched 4 levels deep through the keys
/// providers use for edits (`extractChangedFiles`).
fn extract_changed_files(payload: Option<&Map<String, Value>>) -> Vec<String> {
    let mut files = Vec::new();
    if let Some(data) = payload
        .and_then(|p| p.get("data"))
        .filter(|d| d.is_object())
    {
        collect_changed_files(data, &mut files, 0);
    }
    files
}

fn collect_changed_files(value: &Value, files: &mut Vec<String>, depth: usize) {
    if depth > 4 || files.len() >= MAX_CHANGED_FILES {
        return;
    }
    if let Some(entries) = value.as_array() {
        for entry in entries {
            collect_changed_files(entry, files, depth + 1);
            if files.len() >= MAX_CHANGED_FILES {
                return;
            }
        }
        return;
    }
    let Some(record) = value.as_object() else {
        return;
    };
    for key in [
        "path",
        "filePath",
        "relativePath",
        "filename",
        "newPath",
        "oldPath",
    ] {
        if let Some(path) = record.get(key).and_then(Value::as_str).and_then(trimmed)
            && !files.iter().any(|existing| existing == path)
        {
            files.push(path.to_owned());
        }
    }
    for key in [
        "item",
        "result",
        "input",
        "data",
        "changes",
        "files",
        "edits",
        "patch",
        "patches",
        "operations",
    ] {
        if let Some(nested) = record.get(key) {
            collect_changed_files(nested, files, depth + 1);
            if files.len() >= MAX_CHANGED_FILES {
                return;
            }
        }
    }
}

/// Pins each work row to the identity and timestamp this client first showed it with, so a
/// reconnect that backfills an earlier lifecycle event never moves a visible row
/// (`stabilizeWorkLogEntryPresentation`). Keep one per open thread.
#[derive(Debug, Default)]
pub struct WorkLogPresenter {
    scope: String,
    by_alias: HashMap<String, (String, String)>,
}

impl WorkLogPresenter {
    /// Applies the remembered presentation to `entries` and remembers new ones. A different
    /// `scope` (thread key) starts over.
    pub fn present(&mut self, scope: &str, entries: Vec<WorkLogEntry>) -> Vec<WorkLogEntry> {
        if self.scope != scope {
            self.scope = scope.to_owned();
            self.by_alias.clear();
        }
        entries
            .into_iter()
            .map(|mut entry| {
                let mut aliases: Vec<String> = Vec::with_capacity(3);
                let candidates = [
                    entry.presentation_id.clone(),
                    entry
                        .tool_call_id
                        .as_ref()
                        .map(|call| format!("tool:{call}")),
                    Some(entry.id.clone()),
                ];
                for alias in candidates.into_iter().flatten() {
                    if !aliases.contains(&alias) {
                        aliases.push(alias);
                    }
                }
                let natural_id = entry.presentation_key();
                let (presentation_id, created_at) = aliases
                    .iter()
                    .find_map(|alias| self.by_alias.get(alias).cloned())
                    .unwrap_or_else(|| (natural_id.clone(), entry.created_at.clone()));
                aliases.push(presentation_id.clone());
                for alias in aliases {
                    self.by_alias
                        .insert(alias, (presentation_id.clone(), created_at.clone()));
                }
                if presentation_id != natural_id || created_at != entry.created_at {
                    entry.presentation_id = Some(presentation_id);
                    entry.created_at = created_at;
                }
                entry
            })
            .collect()
    }
}
