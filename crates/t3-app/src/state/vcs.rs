//! Shared `subscribeVcsStatus` streams: one per (environment, cwd), alive while any view wants
//! it (spec 2.12.3 "keep one subscription per (env, cwd)").
//!
//! Views declare what they show with [`VcsStatusStore::set_interest`] under their own owner
//! name (the sidebar passes every rendered row with a branch); the store starts and stops
//! streams for the union. Read with [`VcsStatusStore::status`] and `cx.observe` the store.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Task};
use t3_protocol::{
    EnvironmentId,
    methods::SubscribeVcsStatus,
    vcs::{VcsCwdInput, VcsStatusLocal, VcsStatusRemote, VcsStatusStreamEvent},
};

use super::AppState;

/// Pause before resubscribing after a stream ends or fails.
const RETRY_DELAY: Duration = Duration::from_millis(250);

/// One working copy in one environment.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VcsKey {
    pub environment_id: EnvironmentId,
    pub cwd: String,
}

/// The latest status of a working copy. Either half may be missing until it arrives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VcsStatus {
    pub local: Option<VcsStatusLocal>,
    pub remote: Option<VcsStatusRemote>,
}

struct Entry {
    status: Option<VcsStatus>,
    _stream: Option<Task<()>>,
}

/// The app's VCS status streams.
#[derive(Default)]
pub struct VcsStatusStore {
    entries: HashMap<VcsKey, Entry>,
    interests: HashMap<&'static str, HashSet<VcsKey>>,
}

struct GlobalVcs(Entity<VcsStatusStore>);

impl Global for GlobalVcs {}

impl VcsStatusStore {
    /// The global store, created on first use.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(store) = cx.try_global::<GlobalVcs>() {
            return store.0.clone();
        }
        let store = cx.new(|_| Self::default());
        cx.set_global(GlobalVcs(store.clone()));
        store
    }

    pub fn status(&self, key: &VcsKey) -> Option<&VcsStatus> {
        self.entries
            .get(key)
            .and_then(|entry| entry.status.as_ref())
    }

    /// Replaces what `owner` wants streamed, then starts or stops streams to match.
    pub fn set_interest(
        &mut self,
        owner: &'static str,
        keys: HashSet<VcsKey>,
        cx: &mut Context<Self>,
    ) {
        if self.interests.get(owner) == Some(&keys) {
            return;
        }
        self.interests.insert(owner, keys);
        let wanted: HashSet<&VcsKey> = self.interests.values().flatten().collect();
        self.entries.retain(|key, _| wanted.contains(key));
        let missing: Vec<VcsKey> = wanted
            .into_iter()
            .filter(|key| !self.entries.contains_key(*key))
            .cloned()
            .collect();
        for key in missing {
            let stream = Self::stream(key.clone(), cx);
            self.entries.insert(
                key,
                Entry {
                    status: None,
                    _stream: stream,
                },
            );
        }
    }

    /// Sets a status directly (fixtures and detached environments).
    pub fn set_status(&mut self, key: VcsKey, status: VcsStatus, cx: &mut Context<Self>) {
        let entry = self.entries.entry(key).or_insert(Entry {
            status: None,
            _stream: None,
        });
        entry.status = Some(status);
        cx.notify();
    }

    fn apply(&mut self, key: &VcsKey, event: VcsStatusStreamEvent, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get_mut(key) else {
            return;
        };
        let status = entry.status.get_or_insert_with(VcsStatus::default);
        match event {
            VcsStatusStreamEvent::Snapshot { local, remote } => {
                status.local = Some(local);
                status.remote = remote;
            }
            VcsStatusStreamEvent::LocalUpdated { local } => status.local = Some(local),
            VcsStatusStreamEvent::RemoteUpdated { remote } => status.remote = remote,
            VcsStatusStreamEvent::Unknown => return,
        }
        cx.notify();
    }

    /// Follows one working copy: waits for a connection, subscribes, applies items, and
    /// resubscribes when the stream ends (reconnects end every stream). `None` for detached
    /// environments.
    fn stream(key: VcsKey, cx: &mut Context<Self>) -> Option<Task<()>> {
        let client = AppState::global(cx)
            .read(cx)
            .environment(&key.environment_id, cx)?
            .read(cx)
            .client()?
            .clone();
        Some(cx.spawn(async move |this, cx| {
            let mut status = client.status();
            loop {
                while !status.borrow_and_update().is_connected() {
                    if status.changed().await.is_err() {
                        return;
                    }
                }
                let input = VcsCwdInput {
                    cwd: key.cwd.clone(),
                };
                if let Ok(mut subscription) = client.subscribe::<SubscribeVcsStatus>(&input) {
                    while let Some(Ok(event)) = subscription.next().await {
                        if this
                            .update(cx, |this, cx| this.apply(&key, event, cx))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                cx.background_executor().timer(RETRY_DELAY).await;
            }
        }))
    }
}
