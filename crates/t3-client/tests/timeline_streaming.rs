//! Feeds every state of the recorded token-streaming turns (`fixtures/stream-scenarios.jsonl`)
//! through the chat timeline (`t3_logic::timeline::TimelineModel`), the way the chat view
//! does while a reply streams in.
//!
//! Failure modes:
//! 1. A row diff is not a valid splice of the previous rows, so the virtual list's item count
//!    drifts from the rows it renders.
//! 2. A streaming delta inserts or removes rows instead of only remeasuring the growing
//!    assistant row (the list would lose its scroll anchor on every token).
//! 3. Rows shift under the reader mid-turn: the user message leaves the top, or a visible
//!    row's key changes.
//! 4. The running turn folds before it settles, or the settled turn does not fold.
//! 5. The incremental derivation (presenter state, cached work entries) ends up different
//!    from a fresh derivation of the final state.

mod support;

use std::{collections::HashSet, sync::Arc};

use support::Transcript;
use t3_client::ThreadState;
use t3_logic::timeline::{
    RowKind, SessionPhase, TimelineInput, TimelineModel, TimelineRow, active_work_started_at,
};
use t3_protocol::ThreadId;

const FIXTURE: &str = include_str!("fixtures/stream-scenarios.jsonl");

/// One timeline update for `state`, as the chat view runs it.
fn update(model: &mut TimelineModel, state: &ThreadState) -> Option<t3_logic::timeline::RowsDiff> {
    let thread = state.thread.as_ref().expect("replayed thread has data");
    let working = matches!(
        SessionPhase::of(thread.session.as_ref()),
        SessionPhase::Running | SessionPhase::Connecting
    );
    let started = active_work_started_at(thread.latest_turn.as_ref(), thread.session.as_ref(), None);
    let none = HashSet::new();
    let groups = HashSet::new();
    model.update(TimelineInput {
        thread,
        scope: "env:thread",
        optimistic: &[],
        expanded_turns: &none,
        expanded_work_groups: &groups,
        is_working: working,
        active_turn_started_at: started.as_deref(),
    })
}

fn keys(rows: &[Arc<TimelineRow>]) -> Vec<String> {
    rows.iter().map(|row| row.id.clone()).collect()
}

#[test]
fn streaming_turns_update_the_timeline_by_valid_splices() {
    let transcript = Transcript::parse(FIXTURE);
    for scenario in ["showcase", "approval", "question", "plan"] {
        let thread_id = ThreadId::from(format!("thread-rec-{scenario}"));
        let mut model = TimelineModel::default();
        // The list's item keys, maintained only through the reported diffs.
        let mut list: Vec<String> = Vec::new();
        let mut remeasure_only = 0;
        let mut folded_while_running = false;
        let (final_state, _) = transcript.replay_thread(&thread_id, |state| {
            let before = list.clone();
            let Some(diff) = update(&mut model, state) else {
                return;
            };
            let rows = model.rows();
            let replacement: Vec<String> = keys(&rows[diff.new.clone()]);
            list.splice(diff.old.clone(), replacement);
            assert_eq!(list, keys(rows), "{scenario}: diff is not a splice");
            if diff.same_keys {
                remeasure_only += 1;
                assert_eq!(before, list, "{scenario}: same_keys diff changed keys");
            }
            if let Some(first) = rows.first() {
                assert!(
                    matches!(&first.kind, RowKind::Message(m) if m.message.role.as_str() == "user"),
                    "{scenario}: the user message left the top"
                );
            }
            let thread = state.thread.as_ref().unwrap();
            let running = SessionPhase::of(thread.session.as_ref()) == SessionPhase::Running;
            if running && rows.iter().any(|r| matches!(r.kind, RowKind::TurnFold { .. })) {
                folded_while_running = true;
            }
        });
        assert!(!folded_while_running, "{scenario}: folded the running turn");

        // The settled turn folds, and the incremental rows equal a fresh derivation.
        let rows = model.rows().to_vec();
        assert!(
            rows.iter().any(|r| matches!(r.kind, RowKind::TurnFold { .. })),
            "{scenario}: the settled turn did not fold"
        );
        let mut fresh = TimelineModel::default();
        update(&mut fresh, &final_state);
        let fresh: Vec<&TimelineRow> = fresh.rows().iter().map(Arc::as_ref).collect();
        let ours: Vec<&TimelineRow> = rows.iter().map(Arc::as_ref).collect();
        assert_eq!(ours, fresh, "{scenario}: incremental rows differ from a fresh derivation");

        if scenario == "showcase" {
            assert!(
                remeasure_only > 20,
                "showcase: only {remeasure_only} updates were pure remeasures"
            );
        }
    }
}
