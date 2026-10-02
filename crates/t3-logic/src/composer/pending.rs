//! Answer state of the pending user-input panel and the context-window meter
//! (`pendingUserInput.ts`, `ComposerPendingApprovalPanel.tsx`, `lib/contextWindow.ts`).
//!
//! The requests themselves come from `t3_client::pending_requests`; questions are passed here as
//! `(id, multi_select)` pairs so this module stays free of client types.

use std::{collections::BTreeMap, sync::Arc};

use serde_json::Value;
use t3_protocol::orchestration::OrchestrationThreadActivity;

/// The approval panel's summary for a request kind (`command`, `file-read`, anything else).
pub fn approval_summary(request_kind: &str) -> &'static str {
    match request_kind {
        "command" => "Command approval requested",
        "file-read" => "File-read approval requested",
        _ => "File-change approval requested",
    }
}

/// The in-progress answer to one question (`PendingUserInputDraftAnswer`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftAnswer {
    pub selected: Vec<String>,
    /// Typed in the editor; overrides the selection when non-blank.
    pub custom: String,
}

/// A question's answer, if it has one: the trimmed custom text, else the selected label(s)
/// (`resolvePendingUserInputAnswer`).
pub fn resolve_answer(multi_select: bool, draft: Option<&DraftAnswer>) -> Option<Value> {
    let draft = draft?;
    let custom = draft.custom.trim();
    if !custom.is_empty() {
        return Some(Value::String(custom.to_owned()));
    }
    let mut labels: Vec<String> = Vec::new();
    for label in draft.selected.iter().map(|label| label.trim()) {
        if !label.is_empty() && !labels.iter().any(|existing| existing == label) {
            labels.push(label.to_owned());
        }
    }
    if multi_select {
        (!labels.is_empty()).then(|| Value::Array(labels.into_iter().map(Value::String).collect()))
    } else {
        labels.into_iter().next().map(Value::String)
    }
}

/// Picks an option (`togglePendingUserInputOptionSelection`): multi-select toggles it,
/// single-select replaces the selection. Either clears the custom text.
pub fn toggle_option(multi_select: bool, draft: &mut DraftAnswer, label: &str) {
    draft.custom.clear();
    if multi_select {
        if let Some(index) = draft.selected.iter().position(|existing| existing == label) {
            draft.selected.remove(index);
        } else {
            draft.selected.push(label.to_owned());
        }
    } else {
        draft.selected = vec![label.to_owned()];
    }
}

/// Typing a custom answer drops the selection once the text is non-blank
/// (`setPendingUserInputCustomAnswer`).
pub fn set_custom_answer(draft: &mut DraftAnswer, text: &str) {
    draft.custom = text.to_owned();
    if !text.trim().is_empty() {
        draft.selected.clear();
    }
}

/// A question as the answer helpers see it: its id and whether it takes several options.
pub type QuestionShape<'a> = (&'a str, bool);

/// Every question's answer keyed by id, or `None` while any is unanswered
/// (`buildPendingUserInputAnswers`).
pub fn build_answers(
    questions: &[QuestionShape<'_>],
    drafts: &BTreeMap<String, DraftAnswer>,
) -> Option<BTreeMap<String, Value>> {
    questions
        .iter()
        .map(|&(id, multi_select)| {
            resolve_answer(multi_select, drafts.get(id)).map(|answer| (id.to_owned(), answer))
        })
        .collect()
}

/// Panel state for the active question (`derivePendingUserInputProgress`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub question_index: usize,
    pub is_last: bool,
    /// Every question has an answer.
    pub is_complete: bool,
    /// The active question has an answer.
    pub can_advance: bool,
    /// The active question's custom text is non-blank.
    pub using_custom: bool,
}

pub fn progress(
    questions: &[QuestionShape<'_>],
    drafts: &BTreeMap<String, DraftAnswer>,
    question_index: usize,
) -> Progress {
    let index = question_index.min(questions.len().saturating_sub(1));
    let active = questions.get(index);
    let draft = active.and_then(|(id, _)| drafts.get(*id));
    Progress {
        question_index: index,
        is_last: questions.is_empty() || index + 1 >= questions.len(),
        is_complete: build_answers(questions, drafts).is_some(),
        can_advance: active.is_some_and(|&(_, multi)| resolve_answer(multi, draft).is_some()),
        using_custom: draft.is_some_and(|draft| !draft.custom.trim().is_empty()),
    }
}

/// The primary button label of the user-input panel (`formatPendingPrimaryActionLabel`).
pub fn primary_action_label(
    compact: bool,
    is_last: bool,
    responding: bool,
    question_index: usize,
) -> &'static str {
    match (responding, compact, is_last) {
        (true, _, _) => "Submitting...",
        (false, true, true) => "Submit",
        (false, true, false) => "Next",
        (false, false, false) => "Next question",
        (false, false, true) if question_index > 0 => "Submit answers",
        (false, false, true) => "Submit answer",
    }
}

/// Context window usage from the newest `context-window.updated` activity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextWindow {
    pub used_tokens: f64,
    pub max_tokens: Option<f64>,
    pub total_processed_tokens: Option<f64>,
    /// 0–100, when the max is known.
    pub used_percentage: Option<f64>,
    pub compacts_automatically: bool,
}

/// The newest valid context-window snapshot (`deriveLatestContextWindowSnapshot`).
pub fn latest_context_window(
    activities: &[Arc<OrchestrationThreadActivity>],
) -> Option<ContextWindow> {
    activities.iter().rev().find_map(|activity| {
        if activity.kind != "context-window.updated" {
            return None;
        }
        let payload = &activity.payload;
        let number = |key: &str| {
            payload
                .get(key)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
        };
        let used_tokens = number("usedTokens").filter(|used| *used >= 0.0)?;
        let max_tokens = number("maxTokens");
        Some(ContextWindow {
            used_tokens,
            max_tokens,
            total_processed_tokens: number("totalProcessedTokens"),
            used_percentage: max_tokens
                .filter(|max| *max > 0.0)
                .map(|max| (used_tokens / max * 100.0).min(100.0)),
            compacts_automatically: payload
                .get("compactsAutomatically")
                .and_then(Value::as_bool)
                == Some(true),
        })
    })
}

/// Token count as the meter shows it: 950, 1.2k, 12k, 1.2m (`formatContextWindowTokens`).
pub fn format_tokens(tokens: f64) -> String {
    // JS `toFixed(1)` rounds halves away from zero; Rust's `{:.1}` rounds them to even.
    let one_decimal = |value: f64, suffix: &str| {
        let text = format!("{:.1}", (value * 10.0).round() / 10.0);
        format!("{}{suffix}", text.strip_suffix(".0").unwrap_or(&text))
    };
    if !tokens.is_finite() {
        "0".into()
    } else if tokens < 1_000.0 {
        format!("{}", tokens.round() as i64)
    } else if tokens < 10_000.0 {
        one_decimal(tokens / 1_000.0, "k")
    } else if tokens < 1_000_000.0 {
        format!("{}k", (tokens / 1_000.0).round() as i64)
    } else {
        one_decimal(tokens / 1_000_000.0, "m")
    }
}
