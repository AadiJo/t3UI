//! Thread and project ordering (`packages/client-runtime/src/state/threadSort.ts`,
//! `web/components/Sidebar.logic.ts`).

use std::cmp::Ordering;

use t3_protocol::orchestration::OrchestrationThreadShell;

use crate::{settings::ThreadSortOrder, time::parse_timestamp};

/// Sort key of a thread. `None` sorts last (the web's `-Infinity`).
pub fn thread_sort_timestamp(
    thread: &OrchestrationThreadShell,
    order: ThreadSortOrder,
) -> Option<i64> {
    let first_parseable = |values: [&str; 2]| values.into_iter().find_map(parse_timestamp);
    match order {
        ThreadSortOrder::CreatedAt => first_parseable([&thread.created_at, &thread.updated_at]),
        ThreadSortOrder::UpdatedAt => match thread.latest_user_message_at.as_deref() {
            // A present but unparseable value does not fall back (web parity).
            Some(latest) if !latest.is_empty() => parse_timestamp(latest),
            _ => first_parseable([&thread.updated_at, &thread.created_at]),
        },
    }
}

/// Newest first, ties broken by id descending (web `sortThreads`).
pub fn compare_threads(
    left: &OrchestrationThreadShell,
    right: &OrchestrationThreadShell,
    order: ThreadSortOrder,
) -> Ordering {
    thread_sort_timestamp(right, order)
        .cmp(&thread_sort_timestamp(left, order))
        .then_with(|| right.id.as_str().cmp(left.id.as_str()))
}

/// Approximates `String.prototype.localeCompare` for titles: case-insensitive first, then exact.
pub fn locale_compare(left: &str, right: &str) -> Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

/// Reorders `items` so those matching `preferred` come first in that order; the rest keep their
/// order (web `orderItemsByPreferredIds`). `preference_ids` lists every id an item answers to.
pub fn order_by_preferred<T, F>(items: Vec<T>, preferred: &[String], preference_ids: F) -> Vec<T>
where
    F: Fn(&T) -> Vec<String>,
{
    if preferred.is_empty() {
        return items;
    }
    let ids: Vec<Vec<String>> = items.iter().map(&preference_ids).collect();
    let mut emitted = vec![false; items.len()];
    let mut order = Vec::with_capacity(items.len());
    for wanted in preferred {
        if let Some(index) =
            (0..items.len()).find(|&index| !emitted[index] && ids[index].contains(wanted))
        {
            emitted[index] = true;
            order.push(index);
        }
    }
    order.extend((0..items.len()).filter(|&index| !emitted[index]));
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|index| slots[index].take())
        .collect()
}
