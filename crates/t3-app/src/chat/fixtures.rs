//! Thread detail for detached (fixture) environments, which have no server to subscribe to.
//! Snapshot scenes install recorded `ThreadState`s here; a chat view on a detached environment
//! reads its thread from this table.

use std::{collections::HashMap, sync::Arc};

use gpui_kit::{App, Global};
use t3_client::ThreadState;
use t3_logic::ThreadRef;

#[derive(Default)]
struct ThreadFixtures(HashMap<ThreadRef, Arc<ThreadState>>);

impl Global for ThreadFixtures {}

/// Makes `state` the detail of `thread` for chat views built afterwards.
pub fn install(thread: ThreadRef, state: ThreadState, cx: &mut App) {
    cx.default_global::<ThreadFixtures>()
        .0
        .insert(thread, Arc::new(state));
}

/// The installed detail of `thread`, if any.
pub(super) fn thread(thread: &ThreadRef, cx: &App) -> Option<Arc<ThreadState>> {
    cx.try_global::<ThreadFixtures>()?.0.get(thread).cloned()
}
