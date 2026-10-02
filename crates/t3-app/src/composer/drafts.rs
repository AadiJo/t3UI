//! The draft store: composer content per thread or draft, draft sessions for threads that have not
//! started, and the sticky model choice. Persisted to `drafts.json` (shape in
//! `t3_logic::composer::draft`), written 300ms after the last change and flushed at quit.
//!
//! It also owns new-thread requests (`AppEvent::NewThread`): it reuses the project's open draft
//! or creates one, then navigates to `Route::Draft`.
//!
//! ```ignore
//! let drafts = DraftStore::global(cx);
//! let prompt = drafts.read(cx).draft(&key).map(|draft| draft.prompt.clone());
//! drafts.update(cx, |drafts, cx| drafts.update_draft(&key, |draft| draft.prompt = text, cx));
//! ```

use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, SharedString, Task};
use t3_logic::{
    ThreadRef,
    composer::draft::{ComposerDraft, DraftEnvMode, DraftThread, DraftsFile, PromotedThread},
};
use t3_protocol::{
    ProviderInstanceId, ThreadId,
    orchestration::{ModelSelection, ThreadEnvMode},
};

use crate::state::{AppEvent, AppState, DraftId, NewThreadRequest, Route, Store};

const DRAFTS_FILE: &str = "drafts.json";
/// The web debounces draft writes by 300ms.
const WRITE_DELAY: Duration = Duration::from_millis(300);

/// Where a composer's content lives: a server thread or a draft session.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ComposerTarget {
    Thread(ThreadRef),
    Draft(DraftId),
}

impl ComposerTarget {
    /// The `draftsByThreadKey` key: the draft id, or `<environmentId>:<threadId>`.
    pub fn key(&self) -> String {
        match self {
            Self::Thread(thread) => thread.key(),
            Self::Draft(draft) => draft.0.to_string(),
        }
    }
}

/// Persisted drafts. One global entity; see the module docs.
pub struct DraftStore {
    store: Store,
    file: DraftsFile,
    /// `file.sticky_model_selection_by_provider` as a slice for model resolution.
    sticky_cache: Vec<ModelSelection>,
    pending_write: Option<Task<()>>,
    _subscriptions: Vec<gpui_kit::Subscription>,
}

struct GlobalDrafts(Entity<DraftStore>);

impl Global for GlobalDrafts {}

impl DraftStore {
    /// Loads `drafts.json` from `store`, installs the global, and starts handling new-thread
    /// requests from [`AppState`]. Call once after `AppState::init`.
    pub fn init(store: Store, cx: &mut App) -> Entity<Self> {
        let file = store
            .read(DRAFTS_FILE)
            .map(|json| DraftsFile::from_json(&json))
            .unwrap_or_default();
        let app_state = AppState::global(cx);
        let sticky_cache = file
            .sticky_model_selection_by_provider
            .values()
            .cloned()
            .collect();
        let drafts = cx.new(|cx| Self {
            store,
            file,
            sticky_cache,
            pending_write: None,
            _subscriptions: vec![
                cx.subscribe(&app_state, |this: &mut Self, _, event: &AppEvent, cx| {
                    if let AppEvent::NewThread(request) = event {
                        this.open_draft(request, cx);
                    }
                }),
                cx.on_app_quit(|this: &mut Self, _| {
                    this.flush();
                    async {}
                }),
            ],
        });
        cx.set_global(GlobalDrafts(drafts.clone()));
        drafts
    }

    /// The global store. Before [`DraftStore::init`] (tests, scenes that skip it), an in-memory
    /// store is created on first use.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(drafts) = cx.try_global::<GlobalDrafts>() {
            return drafts.0.clone();
        }
        Self::init(Store::Memory, cx)
    }

    /// The global store if it exists. Reads during render use this so they never create it.
    pub fn global_ref(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalDrafts>()
            .map(|global| global.0.clone())
    }

    /// The draft for `key`, if any.
    pub fn draft(&self, key: &str) -> Option<&ComposerDraft> {
        self.file.drafts_by_thread_key.get(key)
    }

    /// Edits (creating if needed) the draft for `key` and schedules a write.
    pub fn update_draft(
        &mut self,
        key: &str,
        edit: impl FnOnce(&mut ComposerDraft),
        cx: &mut Context<Self>,
    ) {
        let draft = self
            .file
            .drafts_by_thread_key
            .entry(key.to_owned())
            .or_default();
        let before = draft.clone();
        edit(draft);
        if *draft != before {
            self.schedule_write(cx);
        }
    }

    /// The draft session behind a `/draft/<id>` route.
    pub fn draft_thread(&self, id: &DraftId) -> Option<&DraftThread> {
        self.file.draft_threads_by_thread_key.get(id.0.as_ref())
    }

    /// Edits a draft session (workspace mode, branch) and schedules a write.
    pub fn update_draft_thread(
        &mut self,
        id: &DraftId,
        edit: impl FnOnce(&mut DraftThread),
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.file.draft_threads_by_thread_key.get_mut(id.0.as_ref()) {
            let before = thread.clone();
            edit(thread);
            if *thread != before {
                self.schedule_write(cx);
            }
        }
    }

    /// The last model picked in any composer, which seeds new drafts.
    pub fn sticky(&self) -> (&[ModelSelection], Option<&ProviderInstanceId>) {
        (
            &self.sticky_cache,
            self.file.sticky_active_provider.as_ref(),
        )
    }

    /// Records a model pick as the sticky choice.
    pub fn set_sticky(&mut self, selection: ModelSelection, cx: &mut Context<Self>) {
        self.file.sticky_active_provider = Some(selection.instance_id.clone());
        self.file
            .sticky_model_selection_by_provider
            .insert(selection.instance_id.to_string(), selection);
        self.schedule_write(cx);
    }

    /// Marks a draft as having become `thread` after its first send. The route swaps once the
    /// server thread shows up (see [`DraftStore::finalize_promoted`]).
    pub fn mark_promoted(&mut self, id: &DraftId, thread: &ThreadRef, cx: &mut Context<Self>) {
        self.update_draft_thread(
            id,
            |draft| {
                draft.promoted_to = Some(PromotedThread {
                    environment_id: thread.environment_id.clone(),
                    thread_id: thread.thread_id.clone(),
                })
            },
            cx,
        );
    }

    /// Drops a promoted draft session and its project mapping; the composer content moves to the
    /// server thread's key.
    pub fn finalize_promoted(&mut self, id: &DraftId, thread: &ThreadRef, cx: &mut Context<Self>) {
        let key = id.0.to_string();
        if self.file.draft_threads_by_thread_key.remove(&key).is_none() {
            return;
        }
        self.file
            .logical_project_draft_thread_key_by_logical_project_key
            .retain(|_, draft| *draft != key);
        if let Some(draft) = self.file.drafts_by_thread_key.remove(&key) {
            self.file
                .drafts_by_thread_key
                .entry(thread.key())
                .or_insert(draft);
        }
        self.schedule_write(cx);
    }

    /// Reuses the project's open draft or creates one, applies the request's branch and mode,
    /// and navigates to it (web `openOrReuseProjectDraftThread`). Runs on
    /// `AppEvent::NewThread`; returns the draft's id.
    pub fn open_draft(&mut self, request: &NewThreadRequest, cx: &mut Context<Self>) -> DraftId {
        let project_key = request.project.key();
        let reusable = self
            .file
            .logical_project_draft_thread_key_by_logical_project_key
            .get(&project_key)
            .filter(|key| {
                self.file
                    .draft_threads_by_thread_key
                    .get(*key)
                    .is_some_and(|draft| draft.promoted_to.is_none())
            })
            .cloned();
        let env_mode = match request.env_mode {
            Some(ThreadEnvMode::Worktree) => Some(DraftEnvMode::Worktree),
            Some(ThreadEnvMode::Local) => Some(DraftEnvMode::Local),
            _ => None,
        };
        let key = reusable.unwrap_or_else(|| {
            let key = ThreadId::random().to_string();
            self.file.draft_threads_by_thread_key.insert(
                key.clone(),
                DraftThread {
                    thread_id: ThreadId::random(),
                    environment_id: request.project.environment_id.clone(),
                    project_id: request.project.project_id.clone(),
                    logical_project_key: project_key.clone(),
                    created_at: t3_client::commands::now(),
                    branch: None,
                    worktree_path: None,
                    env_mode: DraftEnvMode::Local,
                    start_from_origin: false,
                    promoted_to: None,
                },
            );
            self.file
                .logical_project_draft_thread_key_by_logical_project_key
                .insert(project_key, key.clone());
            key
        });
        if let Some(draft) = self.file.draft_threads_by_thread_key.get_mut(&key) {
            if request.branch.is_some() || request.worktree_path.is_some() {
                draft.branch = request.branch.clone();
                draft.worktree_path = request.worktree_path.clone();
            }
            if let Some(mode) = env_mode {
                draft.env_mode = mode;
            }
            if let Some(start) = request.start_from_origin {
                draft.start_from_origin = start;
            }
        }
        self.schedule_write(cx);
        let id = DraftId(SharedString::from(key));
        let route = Route::Draft(id.clone());
        AppState::global(cx).update(cx, |state, cx| state.navigate(route, cx));
        id
    }

    fn schedule_write(&mut self, cx: &mut Context<Self>) {
        self.sticky_cache = self
            .file
            .sticky_model_selection_by_provider
            .values()
            .cloned()
            .collect();
        cx.notify();
        let delay = cx.background_executor().timer(WRITE_DELAY);
        self.pending_write = Some(cx.spawn(async move |this, cx| {
            delay.await;
            this.update(cx, |this, cx| {
                this.pending_write = None;
                this.store
                    .write(DRAFTS_FILE, this.file.to_json(), cx)
                    .detach();
            })
            .ok();
        }));
    }

    fn flush(&mut self) {
        if self.pending_write.take().is_some() {
            self.store.write_now(DRAFTS_FILE, &self.file.to_json());
        }
    }
}
