//! What the plan surface shows, derived from the thread like the fork does: the active plan's
//! steps (`session-logic.ts` `deriveActivePlanState`), the proposed plan
//! (`findSidebarProposedPlan`), and the export filename (`proposedPlan.ts`). Title and body
//! text come from `t3_logic::timeline::plan`, shared with the timeline's plan card.
//!
//! Plain Rust with no GPUI, so it can move to `t3-logic` if another view needs it.

use std::sync::Arc;

use serde_json::Value;
use t3_logic::timeline::{compare_activities, is_latest_turn_settled, plan::plan_title};
use t3_protocol::{
    TurnId,
    orchestration::{OrchestrationProposedPlan, OrchestrationThread, OrchestrationThreadActivity},
};

/// A plan step's progress. Unknown statuses read as pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanStep {
    pub step: String,
    pub status: StepStatus,
}

/// The newest `turn.plan.updated` activity's checklist (`ActivePlanState`).
#[derive(Clone, Debug, PartialEq)]
pub struct ActivePlan {
    pub created_at: String,
    pub turn_id: Option<TurnId>,
    pub explanation: Option<String>,
    pub steps: Vec<PlanStep>,
}

/// Everything the surface renders for one thread.
#[derive(Clone, Debug, Default)]
pub struct PlanView {
    pub active_plan: Option<ActivePlan>,
    pub proposed_plan: Option<Arc<OrchestrationProposedPlan>>,
}

impl PlanView {
    /// Derives the view. `source_plans` are the proposed plans of the thread named by
    /// `latestTurn.sourceProposedPlan` when that is another thread (its detail must be open).
    pub fn derive(
        thread: &OrchestrationThread,
        source_plans: Option<&[Arc<OrchestrationProposedPlan>]>,
    ) -> Self {
        let latest_turn_id = thread.latest_turn.as_ref().map(|turn| &turn.turn_id);
        Self {
            active_plan: derive_active_plan(&thread.activities, latest_turn_id),
            proposed_plan: sidebar_proposed_plan(thread, source_plans),
        }
    }

    /// The header badge (`planSidebarLabel`). The fork pins the composer to the default
    /// interaction mode, so only a proposed plan makes it "Plan".
    pub fn label(&self) -> &'static str {
        if self.proposed_plan.is_some() {
            "Plan"
        } else {
            "Tasks"
        }
    }
}

/// `deriveActivePlanState`: the latest plan update of the latest turn, else of any turn (so
/// TodoWrite tasks persist across follow-ups). `None` when that update has no valid steps.
pub fn derive_active_plan(
    activities: &[Arc<OrchestrationThreadActivity>],
    latest_turn_id: Option<&TurnId>,
) -> Option<ActivePlan> {
    let mut updates: Vec<&OrchestrationThreadActivity> = activities
        .iter()
        .map(Arc::as_ref)
        .filter(|activity| activity.kind == "turn.plan.updated")
        .collect();
    updates.sort_by(|left, right| compare_activities(left, right));
    let latest = latest_turn_id
        .and_then(|turn_id| {
            updates
                .iter()
                .rev()
                .find(|activity| activity.turn_id.as_ref() == Some(turn_id))
        })
        .or(updates.last())?;

    let steps: Vec<PlanStep> = latest
        .payload
        .get("plan")?
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let step = entry.get("step")?.as_str()?.to_owned();
            let status = match entry.get("status").and_then(Value::as_str) {
                Some("completed") => StepStatus::Completed,
                Some("inProgress") => StepStatus::InProgress,
                _ => StepStatus::Pending,
            };
            Some(PlanStep { step, status })
        })
        .collect();
    if steps.is_empty() {
        return None;
    }
    Some(ActivePlan {
        created_at: latest.created_at.clone(),
        turn_id: latest.turn_id.clone(),
        explanation: latest
            .payload
            .get("explanation")
            .and_then(Value::as_str)
            .map(str::to_owned),
        steps,
    })
}

/// `findLatestProposedPlan`: the newest plan (by `updatedAt`, then id) of the latest turn,
/// else the newest of any turn.
pub fn find_latest_proposed_plan(
    plans: &[Arc<OrchestrationProposedPlan>],
    latest_turn_id: Option<&TurnId>,
) -> Option<Arc<OrchestrationProposedPlan>> {
    let newest = |plans: &mut dyn Iterator<Item = &Arc<OrchestrationProposedPlan>>| {
        plans
            .max_by(|left, right| {
                left.updated_at
                    .cmp(&right.updated_at)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .cloned()
    };
    latest_turn_id
        .and_then(|turn_id| {
            newest(
                &mut plans
                    .iter()
                    .filter(|plan| plan.turn_id.as_ref() == Some(turn_id)),
            )
        })
        .or_else(|| newest(&mut plans.iter()))
}

/// `findSidebarProposedPlan`: while a turn implementing a plan runs, the plan it implements;
/// otherwise the thread's latest proposed plan.
pub fn sidebar_proposed_plan(
    thread: &OrchestrationThread,
    source_plans: Option<&[Arc<OrchestrationProposedPlan>]>,
) -> Option<Arc<OrchestrationProposedPlan>> {
    let latest_turn = thread.latest_turn.as_ref();
    if !is_latest_turn_settled(latest_turn, thread.session.as_ref())
        && let Some(source) = latest_turn.and_then(|turn| turn.source_proposed_plan.as_ref())
    {
        let plans = if source.thread_id == thread.id {
            Some(thread.proposed_plans.as_slice())
        } else {
            source_plans
        };
        if let Some(plan) = plans
            .into_iter()
            .flatten()
            .find(|plan| plan.id == source.plan_id)
        {
            return Some(plan.clone());
        }
    }
    find_latest_proposed_plan(
        &thread.proposed_plans,
        latest_turn.map(|turn| &turn.turn_id),
    )
}

/// `buildProposedPlanMarkdownFilename`: the title as a lowercase dashed slug plus `.md`.
pub fn plan_markdown_filename(markdown: &str) -> String {
    let mut slug = String::new();
    for ch in plan_title(markdown)
        .unwrap_or("plan")
        .to_lowercase()
        .chars()
    {
        if "`'\".,!?()[]{}".contains(ch) {
            continue;
        }
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    format!("{}.md", if slug.is_empty() { "plan" } else { slug })
}

/// `normalizePlanMarkdownForExport`: exactly one trailing newline.
pub fn normalize_plan_markdown_for_export(markdown: &str) -> String {
    format!("{}\n", markdown.trim_end())
}
