//! Section ordering for the inbox sidebar (client-runtime `state/threadSort.ts` and the fork's
//! `sortInboxThreadsByReturn`): settled history, user-arranged pinned and active order keys,
//! and the fractional order-key math behind drag reordering.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use t3_protocol::orchestration::OrchestrationThreadShell;

use crate::time::{EpochMillis, parse_timestamp};

/// A sortable row: a thread shell plus the environment it lives in. Thread ids are unique only
/// within one environment, so ties break on both.
pub trait ThreadRow {
    fn shell(&self) -> &OrchestrationThreadShell;
    fn environment(&self) -> &str {
        ""
    }
}

impl ThreadRow for Arc<OrchestrationThreadShell> {
    fn shell(&self) -> &OrchestrationThreadShell {
        self
    }
}

fn millis(value: Option<&str>) -> Option<EpochMillis> {
    value.and_then(parse_timestamp)
}

/// Id, then environment: the identity tiebreak every section uses.
fn identity<T: ThreadRow>(left: &T, right: &T) -> Ordering {
    left.shell()
        .id
        .as_str()
        .cmp(right.shell().id.as_str())
        .then_with(|| left.environment().cmp(right.environment()))
}

/// The time a settled row sorts and labels by: `settledAt` when valid, else the newest of the
/// latest user message and turn stamps, else `updatedAt`. Malformed stamps are skipped.
pub fn resolve_settled_thread_timestamp(thread: &OrchestrationThreadShell) -> Option<String> {
    if let Some(settled_at) = thread.settled_at.as_deref()
        && parse_timestamp(settled_at).is_some()
    {
        return Some(settled_at.to_owned());
    }
    let turn = thread.latest_turn.as_ref();
    [
        thread.latest_user_message_at.as_deref(),
        turn.map(|t| t.requested_at.as_str()),
        turn.and_then(|t| t.started_at.as_deref()),
        turn.and_then(|t| t.completed_at.as_deref()),
    ]
    .into_iter()
    .flatten()
    .filter_map(|candidate| Some((parse_timestamp(candidate)?, candidate)))
    // First of equal maxima wins, like the web's strict `>`.
    .fold(
        None,
        |best: Option<(EpochMillis, &str)>, (at, candidate)| match best {
            Some((best_at, _)) if best_at >= at => best,
            _ => Some((at, candidate)),
        },
    )
    .map(|(_, candidate)| candidate.to_owned())
    .or_else(|| {
        parse_timestamp(&thread.updated_at)
            .is_some()
            .then(|| thread.updated_at.clone())
    })
}

/// Settled rows are history: newest end of work first, id tiebreak.
pub fn sort_settled_threads<T: ThreadRow>(threads: Vec<T>) -> Vec<T> {
    let mut keyed: Vec<(EpochMillis, T)> = threads
        .into_iter()
        .map(|thread| {
            let at = resolve_settled_thread_timestamp(thread.shell())
                .and_then(|stamp| parse_timestamp(&stamp))
                .unwrap_or(0);
            (at, thread)
        })
        .collect();
    keyed.sort_by(|(left_at, left), (right_at, right)| {
        right_at.cmp(left_at).then_with(|| identity(left, right))
    });
    keyed.into_iter().map(|(_, thread)| thread).collect()
}

/// Pinned block: user-arranged keys first (string order, then identity), keyless threads
/// below, newest created first. Capability only gates dragging, never the sort.
pub fn sort_pinned_threads<T: ThreadRow>(threads: Vec<T>) -> Vec<T> {
    let (mut keyed, mut keyless): (Vec<T>, Vec<T>) = threads
        .into_iter()
        .partition(|thread| thread.shell().pin_order_key.is_some());
    keyed.sort_by(|left, right| {
        left.shell()
            .pin_order_key
            .cmp(&right.shell().pin_order_key)
            .then_with(|| identity(left, right))
    });
    keyless.sort_by(|left, right| {
        let at = |t: &T| parse_timestamp(&t.shell().created_at).unwrap_or(0);
        at(right).cmp(&at(left)).then_with(|| identity(left, right))
    });
    keyed.extend(keyless);
    keyed
}

/// Where an active row without an order key sits: its creation, re-anchored to `unsettledAt`
/// when it re-entered the active list.
fn active_anchor(thread: &OrchestrationThreadShell) -> EpochMillis {
    parse_timestamp(&thread.created_at)
        .unwrap_or(0)
        .max(millis(thread.unsettled_at.as_deref()).unwrap_or(0))
}

/// Active list: new and reopened (keyless) threads lead, newest anchor first; arranged
/// threads follow their saved keys. Activity moves neither group.
pub fn sort_active_threads<T: ThreadRow>(mut threads: Vec<T>) -> Vec<T> {
    threads.sort_by(|left, right| {
        let (l, r) = (left.shell(), right.shell());
        let order = match (&l.active_order_key, &r.active_order_key) {
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(left_key), Some(right_key)) => left_key.cmp(right_key),
            (None, None) => active_anchor(r).cmp(&active_anchor(l)),
        };
        order.then_with(|| identity(left, right))
    });
    threads
}

/// Working-shelf inbox order: newest first by when each thread last came back to the user
/// (creation, reopening, turn request or completion, or a return this client observed).
pub fn sort_inbox_threads_by_return<T: ThreadRow>(
    mut threads: Vec<T>,
    observed_return_at: impl Fn(&T) -> Option<EpochMillis>,
) -> Vec<T> {
    let returned_at = |thread: &T| {
        let shell = thread.shell();
        let turn = shell.latest_turn.as_ref();
        [
            parse_timestamp(&shell.created_at),
            millis(shell.unsettled_at.as_deref()),
            turn.and_then(|t| parse_timestamp(&t.requested_at)),
            turn.and_then(|t| millis(t.completed_at.as_deref())),
            observed_return_at(thread),
        ]
        .into_iter()
        .map(|at| at.unwrap_or(0))
        .max()
        .unwrap_or(0)
    };
    let mut keyed: Vec<(EpochMillis, T)> = threads
        .drain(..)
        .map(|thread| (returned_at(&thread), thread))
        .collect();
    keyed.sort_by(|(left_at, left), (right_at, right)| {
        right_at.cmp(left_at).then_with(|| identity(left, right))
    });
    keyed.into_iter().map(|(_, thread)| thread).collect()
}

// ---------------------------------------------------------------------------------------------
// Fractional order keys: base-26 `a`..`z` strings compared as plain strings. A move writes one
// key to one thread on its own server, so clients converge without touching neighbors.

const DIGITS: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";

fn digit_index(byte: u8) -> usize {
    (byte - b'a') as usize
}

/// Non-empty, only `a`..`z`, and not ending in `a` (no room before such a key).
fn is_valid_key(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase()) && !key.ends_with('a')
}

/// Midpoint of two digit strings read as fractions in (0, 1); `""` is the open bound.
/// Requires `a < b` when `b` is not empty.
fn midpoint(a: &str, b: &str) -> String {
    let (a_bytes, b_bytes) = (a.as_bytes(), b.as_bytes());
    if !b.is_empty() {
        // Skip the common prefix (a missing digit in `a` reads as the minimum digit).
        let mut n = 0;
        while n < b_bytes.len() && a_bytes.get(n).copied().unwrap_or(b'a') == b_bytes[n] {
            n += 1;
        }
        if n > 0 {
            return format!("{}{}", &b[..n], midpoint(a.get(n..).unwrap_or(""), &b[n..]));
        }
    }
    let digit_a = a_bytes.first().map_or(0, |&byte| digit_index(byte));
    let digit_b = b_bytes
        .first()
        .map_or(DIGITS.len(), |&byte| digit_index(byte));
    if digit_b - digit_a > 1 {
        // `Math.round` of a .5 rounds up.
        return (DIGITS[(digit_a + digit_b).div_ceil(2)] as char).to_string();
    }
    if b_bytes.len() > 1 {
        return b[..1].to_owned();
    }
    format!(
        "{}{}",
        DIGITS[digit_a] as char,
        midpoint(a.get(1..).unwrap_or(""), "")
    )
}

/// A key strictly between two neighbors; `None` bounds are the open ends. `None` when existing
/// keys are corrupt or out of order (callers rewrite the section instead).
pub fn pin_order_key_between(before: Option<&str>, after: Option<&str>) -> Option<String> {
    let (a, b) = (before.unwrap_or(""), after.unwrap_or(""));
    if (!a.is_empty() && !is_valid_key(a)) || (!b.is_empty() && !is_valid_key(b)) {
        return None;
    }
    if !b.is_empty() && a >= b {
        return None;
    }
    Some(midpoint(a, b))
}

/// `count` evenly spaced keys in ascending order, widening the key for long lists.
pub fn generate_spread_pin_order_keys(count: usize) -> Vec<String> {
    let base = DIGITS.len() as u128;
    let mut width = 2;
    let mut space = base.pow(2);
    while space <= (count as u128 + 1) * 2 {
        width += 1;
        space *= base;
    }
    let step = space as f64 / (count as f64 + 1.0);
    (0..count)
        .map(|index| {
            let mut value = (step * (index as f64 + 1.0)).round() as u128;
            // Skip values whose low digit is the minimum (a trailing `a`).
            if value.is_multiple_of(base) {
                value += 1;
            }
            let mut key = vec![b'a'; width];
            for slot in key.iter_mut().rev() {
                *slot = DIGITS[(value % base) as usize];
                value /= base;
            }
            String::from_utf8(key).expect("ascii")
        })
        .collect()
}

/// The `(thread id, order key)` writes that realize `ordered_ids` after `moved_id` moved. One
/// write when both neighbors have keys; otherwise the visible section gets fresh spread keys
/// (a one-time materialization). `keys_by_id` may include hidden rows: their keys are never
/// reused, and they are never written. Also used for active reordering with `activeOrderKey`.
pub fn plan_pinned_reorder(
    ordered_ids: &[String],
    keys_by_id: &HashMap<String, Option<String>>,
    moved_id: &str,
) -> Vec<(String, String)> {
    let visible: HashSet<&str> = ordered_ids.iter().map(String::as_str).collect();
    let reserved: HashSet<&str> = keys_by_id
        .iter()
        .filter(|(id, _)| !visible.contains(id.as_str()))
        .filter_map(|(_, key)| key.as_deref())
        .collect();
    let Some(moved_index) = ordered_ids.iter().position(|id| id == moved_id) else {
        return Vec::new();
    };
    let key_of = |index: usize| {
        ordered_ids
            .get(index)
            .and_then(|id| keys_by_id.get(id).cloned().flatten())
    };
    let before_exists = moved_index > 0;
    let after_exists = moved_index + 1 < ordered_ids.len();
    let before_key = before_exists.then(|| key_of(moved_index - 1)).flatten();
    let after_key = after_exists.then(|| key_of(moved_index + 1)).flatten();
    if (!before_exists || before_key.is_some()) && (!after_exists || after_key.is_some()) {
        let mut key = pin_order_key_between(before_key.as_deref(), after_key.as_deref());
        while let Some(candidate) = key.as_deref().filter(|k| reserved.contains(k)) {
            key = pin_order_key_between(Some(candidate), after_key.as_deref());
        }
        if let Some(key) = key {
            return vec![(moved_id.to_owned(), key)];
        }
    }
    let keys: Vec<String> = generate_spread_pin_order_keys(ordered_ids.len() + reserved.len())
        .into_iter()
        .filter(|key| !reserved.contains(key.as_str()))
        .take(ordered_ids.len())
        .collect();
    ordered_ids
        .iter()
        .zip(keys)
        .filter(|(id, key)| keys_by_id.get(*id).cloned().flatten().as_ref() != Some(key))
        .map(|(id, key)| (id.clone(), key))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveDirection {
    Up,
    Down,
}

/// "Move up" / "Move down" as a reorder plan; `None` when the move falls off either end.
pub fn plan_pinned_move(
    ordered_ids: &[String],
    keys_by_id: &HashMap<String, Option<String>>,
    moved_id: &str,
    direction: MoveDirection,
) -> Option<Vec<(String, String)>> {
    let from = ordered_ids.iter().position(|id| id == moved_id)?;
    let to = match direction {
        MoveDirection::Up => from.checked_sub(1)?,
        MoveDirection::Down => Some(from + 1).filter(|&to| to < ordered_ids.len())?,
    };
    let mut order = ordered_ids.to_vec();
    let moved = order.remove(from);
    order.insert(to, moved);
    Some(plan_pinned_reorder(&order, keys_by_id, moved_id))
}
