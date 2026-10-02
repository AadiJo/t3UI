//! The default sidebar's rendered list and drag plans (fork `Sidebar.tsx` `sidebarListItems`,
//! `Sidebar.logic.ts`). Failure modes this covers, written before the tests:
//!
//! 1. With no listed thread at all, the list has no markers (only the empty state renders).
//! 2. Marker order: pinned label, pinned rows, Active label, active placeholder, active rows,
//!    then Working / Snoozed headers only when those shelves have threads, then the Settled
//!    header and placeholder (always), then settled rows.
//! 3. A collapsed shelf renders no rows, except the route thread's own row; collapsed rows take
//!    no part in traversal or jump shortcuts.
//! 4. Settled paging shows `limit` rows, appends the route thread when it is deeper, and counts
//!    the rest as hidden; a pending settle-then-navigate keeps the route row out of a collapsed
//!    shelf.
//! 5. Traversal: no route thread means first (next) / last (previous); the ends go nowhere; a
//!    route thread that is not rendered goes nowhere. Jump targets are 1-based.
//! 6. The first shelf present takes `mt-auto`.
//! 7. Drop targets: a slot is in the section of the nearest marker above it; Working and
//!    Snoozed slots are not targets; orders are read with the lifted row in its new slot.
//! 8. Drop verbs: none within a section or into Working/Snoozed; Pin, Settle, Unpin, Unsettle,
//!    Wake otherwise.
//! 9. Drop plans: back in place is a no-op; pinning from the inbox carries the planned key on
//!    the pin command; settling needs the capability; a time-ordered inbox takes lifecycle-only
//!    moves; a key write to a non-reorderable thread cancels the drop.
//! 10. Forward navigation skips settled, snoozed, and parking rows, wraps, and never returns the
//!     parked thread itself.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::{Value, json};
use t3_protocol::{
    EnvironmentId, environment::ExecutionEnvironmentCapabilities,
    orchestration::OrchestrationThreadShell,
};

use super::{tests::thread, *};
use crate::{time::parse_timestamp, ui_state::UiState};

const NOW: &str = "2026-10-02T12:00:00.000Z";

fn named(id: &str, extra: Value) -> Arc<OrchestrationThreadShell> {
    thread(id, "p", "2026-10-01T00:00:00.000Z", extra)
}

fn caps() -> ExecutionEnvironmentCapabilities {
    serde_json::from_value(json!({
        "threadSettlement": true, "threadSnooze": true, "threadPinning": true,
        "threadPinReorder": true, "threadActiveReorder": true,
    }))
    .unwrap()
}

fn model(threads: Vec<Arc<OrchestrationThreadShell>>, working_shelf: bool) -> InboxModel {
    let env = EnvironmentId::from("e");
    let capabilities = caps();
    let environments = [InboxEnvironment {
        id: &env,
        threads: &threads,
        capabilities: Some(&capabilities),
    }];
    let ui = UiState::default();
    build_inbox(&InboxInputs {
        environments: &environments,
        scope: None,
        working_shelf_enabled: working_shelf,
        now: parse_timestamp(NOW).unwrap(),
        ui: &ui,
        route_thread: None,
        returns: &InboxReturns::default(),
    })
}

fn r(id: &str) -> ThreadRef {
    ThreadRef::new(EnvironmentId::from("e"), id.into())
}

fn list(model: &InboxModel, shelves: ShelfState, route: Option<&ThreadRef>) -> SidebarList {
    build_sidebar_list(SidebarListInputs {
        model,
        shelves,
        route_thread: route,
        settle_navigation_pending: false,
    })
}

/// Items as short strings: markers by name, rows by id.
fn shape(list: &SidebarList) -> Vec<String> {
    list.items
        .iter()
        .map(|item| match item {
            SidebarListItem::Thread { row, .. } => row.thread.id.to_string(),
            SidebarListItem::Marker(marker) => format!("<{marker:?}>"),
        })
        .collect()
}

fn settled(id: &str, at: &str) -> Arc<OrchestrationThreadShell> {
    named(id, json!({"settledOverride": "settled", "settledAt": at}))
}

fn snoozed(id: &str) -> Arc<OrchestrationThreadShell> {
    named(
        id,
        json!({"snoozedUntil": "2026-10-02T15:00:00.000Z", "snoozedAt": "2026-10-02T10:00:00.000Z"}),
    )
}

fn running(id: &str) -> Arc<OrchestrationThreadShell> {
    named(
        id,
        json!({"session": {"threadId": id, "status": "running", "providerName": "codex",
            "runtimeMode": "full-access", "activeTurnId": "t1", "lastError": null,
            "updatedAt": NOW}}),
    )
}

fn pinned(id: &str, key: &str) -> Arc<OrchestrationThreadShell> {
    named(
        id,
        json!({"pinnedAt": "2026-10-01T00:00:00.000Z", "pinOrderKey": key}),
    )
}

#[test]
fn empty_inbox_has_no_markers() {
    let model = model(Vec::new(), false);
    let list = list(&model, ShelfState::default(), None);
    assert!(list.is_empty());
    assert_eq!(list.first_shelf(), None);
}

#[test]
fn markers_frame_each_section() {
    let model = model(
        vec![
            pinned("pin", "m"),
            named("a", json!({})),
            snoozed("zz"),
            settled("s", "2026-10-01T10:00:00.000Z"),
        ],
        false,
    );
    let shelves = ShelfState {
        snoozed_expanded: true,
        settled_expanded: true,
        ..ShelfState::default()
    };
    let list = list(&model, shelves, None);
    assert_eq!(
        shape(&list),
        [
            "<PinnedHeader>",
            "pin",
            "<PinnedDivider>",
            "<ActivePlaceholder>",
            "a",
            "<SnoozedHeader>",
            "zz",
            "<SettledHeader>",
            "<SettledPlaceholder>",
            "s",
        ]
    );
    assert_eq!(list.first_shelf(), Some(SidebarMarker::SnoozedHeader));

    // Only the settled header shows when no shelf has threads.
    let model = super::list_tests::model(vec![named("a", json!({}))], false);
    let list = super::list_tests::list(&model, ShelfState::default(), None);
    assert_eq!(
        shape(&list),
        [
            "<PinnedHeader>",
            "<PinnedDivider>",
            "<ActivePlaceholder>",
            "a",
            "<SettledHeader>",
            "<SettledPlaceholder>",
        ]
    );
    assert_eq!(list.first_shelf(), Some(SidebarMarker::SettledHeader));
}

#[test]
fn collapsed_shelves_keep_only_the_route_row() {
    let model = model(
        vec![
            named("a", json!({})),
            running("w1"),
            running("w2"),
            snoozed("zz"),
            settled("s1", "2026-10-01T10:00:00.000Z"),
            settled("s2", "2026-10-01T09:00:00.000Z"),
        ],
        true,
    );
    let collapsed = list(&model, ShelfState::default(), None);
    assert_eq!(
        collapsed.thread_refs(),
        [r("a")],
        "collapsed shelves render nothing"
    );
    assert_eq!(
        (
            collapsed.working_count,
            collapsed.snoozed_count,
            collapsed.settled_count
        ),
        (2, 1, 2)
    );
    assert_eq!(collapsed.first_shelf(), Some(SidebarMarker::WorkingHeader));

    let route = r("s2");
    let with_route = list(&model, ShelfState::default(), Some(&route));
    assert_eq!(with_route.thread_refs(), [r("a"), r("s2")]);
    assert_eq!(with_route.jump_target(2), Some(r("s2")));
    assert_eq!(with_route.jump_target(3), None);

    let pending = build_sidebar_list(SidebarListInputs {
        model: &model,
        shelves: ShelfState::default(),
        route_thread: Some(&route),
        settle_navigation_pending: true,
    });
    assert_eq!(pending.thread_refs(), [r("a")]);
}

#[test]
fn settled_tail_pages_and_keeps_the_route_row() {
    let threads: Vec<_> = (0..14)
        .map(|index| {
            settled(
                &format!("s{index:02}"),
                &format!("2026-10-01T{:02}:00:00.000Z", 20 - index),
            )
        })
        .collect();
    let model = model(threads, false);
    let shelves = ShelfState {
        settled_expanded: true,
        ..ShelfState::default()
    };
    let list_default = list(&model, shelves, None);
    assert_eq!(list_default.rows().count(), SETTLED_INITIAL_COUNT);
    assert_eq!(list_default.hidden_settled, 4);

    let route = r("s12");
    let with_route = list(&model, shelves, Some(&route));
    assert_eq!(with_route.rows().count(), SETTLED_INITIAL_COUNT + 1);
    assert_eq!(with_route.thread_refs().last(), Some(&route));
    assert_eq!(with_route.hidden_settled, 3);

    let more = ShelfState {
        settled_limit: SETTLED_INITIAL_COUNT + SETTLED_PAGE_COUNT,
        ..shelves
    };
    let all = list(&model, more, None);
    assert_eq!((all.rows().count(), all.hidden_settled), (14, 0));
}

#[test]
fn traversal_and_jumps_follow_the_rendered_order() {
    let model = model(
        vec![
            named("a", json!({"createdAt": "2026-01-03T00:00:00.000Z"})),
            named("b", json!({"createdAt": "2026-01-02T00:00:00.000Z"})),
            named("c", json!({"createdAt": "2026-01-01T00:00:00.000Z"})),
        ],
        false,
    );
    let list = list(&model, ShelfState::default(), None);
    assert_eq!(list.thread_refs(), [r("a"), r("b"), r("c")]);
    assert_eq!(list.adjacent(None, true), Some(r("a")));
    assert_eq!(list.adjacent(None, false), Some(r("c")));
    assert_eq!(list.adjacent(Some(&r("b")), true), Some(r("c")));
    assert_eq!(list.adjacent(Some(&r("b")), false), Some(r("a")));
    assert_eq!(list.adjacent(Some(&r("c")), true), None);
    assert_eq!(list.adjacent(Some(&r("a")), false), None);
    assert_eq!(list.adjacent(Some(&r("missing")), true), None);
    assert_eq!(list.jump_target(1), Some(r("a")));
    assert_eq!(list.jump_target(0), None);
}

#[test]
fn drop_targets_read_the_markers_above_the_slot() {
    let model = model(
        vec![
            pinned("p1", "a"),
            pinned("p2", "b"),
            named("a1", json!({"createdAt": "2026-01-02T00:00:00.000Z"})),
            named("a2", json!({"createdAt": "2026-01-01T00:00:00.000Z"})),
            snoozed("zz"),
            settled("s1", "2026-10-01T10:00:00.000Z"),
        ],
        false,
    );
    let shelves = ShelfState {
        snoozed_expanded: true,
        settled_expanded: true,
        ..ShelfState::default()
    };
    let list = list(&model, shelves, None);
    // <PinnedHeader> p1 p2 <PinnedDivider> <ActivePlaceholder> a1 a2 <SnoozedHeader> zz
    // <SettledHeader> <SettledPlaceholder> s1
    let index_of = |label: &str| shape(&list).iter().position(|item| item == label).unwrap();

    // Lift p1 and drop it after a1: it joins the inbox between a1 and a2.
    let target = resolve_sidebar_drop_target(&list.items, &r("p1"), index_of("a1")).unwrap();
    assert_eq!(target.section, SidebarSection::Active);
    assert_eq!(target.pinned_order, [r("p2")]);
    assert_eq!(target.active_order, [r("a1"), r("p1"), r("a2")]);

    // Lift a2 and drop it at p2's slot: pinned, above p2.
    let target = resolve_sidebar_drop_target(&list.items, &r("a2"), index_of("p2")).unwrap();
    assert_eq!(target.section, SidebarSection::Pinned);
    assert_eq!(target.pinned_order, [r("p1"), r("a2"), r("p2")]);

    // Into the snoozed shelf: not a target.
    assert!(resolve_sidebar_drop_target(&list.items, &r("a1"), index_of("zz")).is_none());

    // Onto the settled tail.
    let target = resolve_sidebar_drop_target(&list.items, &r("a1"), index_of("s1")).unwrap();
    assert_eq!(target.section, SidebarSection::Settled);
}

#[test]
fn drop_verbs() {
    use SidebarDropVerb::*;
    use SidebarSection::*;
    assert_eq!(resolve_sidebar_drop_verb(Active, Some(Active)), None);
    assert_eq!(resolve_sidebar_drop_verb(Active, Some(Snoozed)), None);
    assert_eq!(resolve_sidebar_drop_verb(Active, Some(Working)), None);
    assert_eq!(resolve_sidebar_drop_verb(Active, None), None);
    assert_eq!(resolve_sidebar_drop_verb(Active, Some(Pinned)), Some(Pin));
    assert_eq!(
        resolve_sidebar_drop_verb(Pinned, Some(Settled)),
        Some(Settle)
    );
    assert_eq!(resolve_sidebar_drop_verb(Pinned, Some(Active)), Some(Unpin));
    assert_eq!(
        resolve_sidebar_drop_verb(Settled, Some(Active)),
        Some(Unsettle)
    );
    assert_eq!(resolve_sidebar_drop_verb(Snoozed, Some(Active)), Some(Wake));
}

struct Plan {
    pinned_keys: HashMap<ThreadRef, Option<String>>,
    active_keys: HashMap<ThreadRef, Option<String>>,
    reorderable: HashSet<ThreadRef>,
}

impl Plan {
    fn new() -> Self {
        Self {
            pinned_keys: HashMap::from([
                (r("p1"), Some("h".to_owned())),
                (r("p2"), Some("p".to_owned())),
            ]),
            active_keys: HashMap::from([(r("a1"), None), (r("a2"), None)]),
            reorderable: ["p1", "p2", "a1", "a2"].into_iter().map(r).collect(),
        }
    }

    fn run(
        &self,
        active: &str,
        section: SidebarSection,
        target: SidebarDropTarget,
        time_ordered: bool,
    ) -> SidebarDropPlan {
        let active = r(active);
        plan_sidebar_thread_drop(SidebarDropInputs {
            active: &active,
            active_section: section,
            active_pinned: section == SidebarSection::Pinned,
            active_settled: section == SidebarSection::Settled,
            supports_settlement: true,
            target: &target,
            pinned_order: &[r("p1"), r("p2")],
            pinned_keys: &self.pinned_keys,
            pin_reorderable: &self.reorderable,
            active_order: &[r("a1"), r("a2")],
            active_keys: &self.active_keys,
            active_reorderable: &self.reorderable,
            active_time_ordered: time_ordered,
        })
    }
}

fn target(section: SidebarSection, pinned: &[&str], active: &[&str]) -> SidebarDropTarget {
    SidebarDropTarget {
        section,
        pinned_order: pinned.iter().copied().map(r).collect(),
        active_order: active.iter().copied().map(r).collect(),
    }
}

#[test]
fn drop_plans() {
    use SidebarSection::*;
    let plan = Plan::new();
    // Back in place.
    assert_eq!(
        plan.run(
            "p1",
            Pinned,
            target(Pinned, &["p1", "p2"], &["a1", "a2"]),
            false
        ),
        SidebarDropPlan::None
    );
    // Reorder pinned: one key write between the keyed neighbors... p2 moves above p1.
    let SidebarDropPlan::ReorderPinned { assignments, .. } = plan.run(
        "p2",
        Pinned,
        target(Pinned, &["p2", "p1"], &["a1", "a2"]),
        false,
    ) else {
        panic!("expected a pinned reorder");
    };
    assert_eq!(assignments.len(), 1);
    assert!(assignments[0].1.as_str() < "h");

    // Pin from the inbox between the pins: the pin command carries the new key.
    let SidebarDropPlan::Pin {
        order_key,
        extra_assignments,
        ..
    } = plan.run(
        "a1",
        Active,
        target(Pinned, &["p1", "a1", "p2"], &["a2"]),
        false,
    )
    else {
        panic!("expected a pin");
    };
    let key = order_key.expect("a key between h and p");
    assert!("h" < key.as_str() && key.as_str() < "p");
    assert!(extra_assignments.is_empty());

    // Settle.
    assert_eq!(
        plan.run("a1", Active, target(Settled, &["p1", "p2"], &["a2"]), false),
        SidebarDropPlan::Settle
    );
    // Time-ordered inbox: lifecycle only.
    assert_eq!(
        plan.run(
            "p1",
            Pinned,
            target(Active, &["p2"], &["a1", "p1", "a2"]),
            true
        ),
        SidebarDropPlan::MoveActive {
            order: None,
            assignments: Vec::new(),
            unpin: true,
            unsettle: false,
            unsnooze: false,
        }
    );
    // A write to a thread whose server cannot reorder cancels the drop.
    let mut blocked = Plan::new();
    blocked.reorderable.remove(&r("a2"));
    assert_eq!(
        blocked.run(
            "p1",
            Pinned,
            target(Active, &["p2"], &["a1", "p1", "a2"]),
            false
        ),
        SidebarDropPlan::None
    );
}

#[test]
fn forward_navigation_skips_parked_rows() {
    let model = model(
        vec![
            named("a", json!({"createdAt": "2026-01-03T00:00:00.000Z"})),
            named("b", json!({"createdAt": "2026-01-02T00:00:00.000Z"})),
            named("c", json!({"createdAt": "2026-01-01T00:00:00.000Z"})),
            settled("s", "2026-10-01T10:00:00.000Z"),
        ],
        false,
    );
    let shelves = ShelfState {
        settled_expanded: true,
        ..ShelfState::default()
    };
    let list = list(&model, shelves, None);
    let none = HashSet::new();
    assert_eq!(plan_forward_navigation(&list, &r("b"), &none), Some(r("c")));
    // Wraps past the settled tail back to the top.
    assert_eq!(plan_forward_navigation(&list, &r("c"), &none), Some(r("a")));
    let parking = HashSet::from([r("c")]);
    assert_eq!(
        plan_forward_navigation(&list, &r("b"), &parking),
        Some(r("a"))
    );
    let all = HashSet::from([r("a"), r("c")]);
    assert_eq!(plan_forward_navigation(&list, &r("b"), &all), None);
}
