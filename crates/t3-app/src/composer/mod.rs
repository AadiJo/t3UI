//! The chat composer (`chat.md` sections 4-7): the prompt card with its editor, chips, `@` / `$` /
//! `/` command menu, image attachments, model and traits pickers, send/stop, the pending
//! approval and question panels, and the branch toolbar under it.
//!
//! ChatView embeds one per thread or draft and feeds it the thread detail it already follows:
//!
//! ```ignore
//! let composer = cx.new(|cx| Composer::new(environment, ComposerTarget::Thread(thread), window, cx));
//! // whenever the thread detail changes:
//! composer.update(cx, |composer, cx| composer.set_thread_state(state.clone(), cx));
//! cx.subscribe(&composer, |this, _, event: &ComposerEvent, cx| match event {
//!     ComposerEvent::Sending { .. } => { /* optimistic message, follow the end */ }
//!     ComposerEvent::SendFailed(message) | ComposerEvent::Error(message) => { /* error banner */ }
//!     ..
//! });
//! // render it at the bottom of the chat body, full width; it centers itself (max 768px).
//! ```
//!
//! The prompt string is the source of truth, like the web; see [`editor`] for how chips map onto
//! it. Content is saved per target in the global [`DraftStore`] (`drafts.json`).

mod branch_toolbar;
pub mod chips;
mod command_menu;
mod drafts;
mod editor;
mod footer;
mod model_picker;
mod panels;
mod style;
mod traits;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use gpui_kit::{
    App, AppContext as _, ClipboardEntry, Context, Entity, EventEmitter, ExternalPaths,
    FocusHandle, Focusable, Image, ImageFormat, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Subscription, Task, TaskExt as _, Window,
    base::ElementExt as _,
    base::input::{
        Enter, IndentInline, InputEvent, MoveDown, MoveUp, Paste, Textarea, TextareaState,
    },
    div, img,
    prelude::FluentBuilder as _,
    px,
};
use t3_client::{PendingRequests, ThreadState, commands};
use t3_logic::{
    ThreadRef,
    composer::{
        draft::{DraftEnvMode, DraftImage},
        menu::{self, MenuAction, MenuItem, Replacement},
        pending::{self, ContextWindow, DraftAnswer},
        prompt::{self, InlineTokenKind, Trigger, TriggerKind},
        providers::{self, ModelContext, ResolvedModel},
        send,
    },
};
use t3_protocol::{
    ApprovalRequestId, MessageId, ThreadId,
    commands::{
        BootstrapCreateThread, BootstrapPrepareWorktree, ThreadMetaPatch, TurnAttachment,
        TurnStartBootstrap,
    },
    methods::ProjectsSearchEntries,
    orchestration::{
        ApprovalDecision, AttachmentKind, InteractionMode, ModelSelection, RuntimeMode,
        SessionStatus,
    },
    projects::{ProjectEntry, ProjectSearchEntriesInput},
    server::{ServerConfig, ServerProvider},
};
use t3_ui::{ActiveColors as _, Icon, IconName, tokens::shadow};

use crate::{
    keybindings::ShortcutScope,
    state::{AppEvent, AppState, Environment, Route},
};

pub use branch_toolbar::BranchToolbar;
pub use drafts::{ComposerTarget, DraftStore};
use model_picker::{ModelPicker, ModelPickerEvent};
use style::{EDITOR_LINE_HEIGHT, EDITOR_MIN_HEIGHT, EDITOR_TEXT};

/// Registers the draft store. Call once at startup, after `AppState::init`.
pub fn init(store: crate::state::Store, cx: &mut App) {
    DraftStore::init(store, cx);
}

/// What the composer tells its host.
#[derive(Clone, Debug, PartialEq)]
pub enum ComposerEvent {
    /// A user message is being sent (`thread.turn.start` is in flight). Show it optimistically,
    /// start the working timer, and follow the end (ChatView `begin_local_dispatch`). A
    /// [`ComposerEvent::SendFailed`] follows if the dispatch fails.
    Sending {
        thread: ThreadRef,
        message_id: MessageId,
        text: String,
    },
    /// The send failed; the composer restored the prompt. Show the message in the thread error
    /// banner and drop the optimistic message (ChatView `end_local_dispatch`).
    SendFailed(SharedString),
    /// A draft's first turn created this server thread.
    ThreadStarted(ThreadRef),
    /// Something else failed (attachment limits, approval reply); show it in the error banner.
    Error(SharedString),
    /// The footer's Plan/Tasks toggle.
    TogglePlanSidebar,
}

/// An attached image waiting to be sent.
#[derive(Clone)]
struct ComposerImage {
    draft: DraftImage,
    bytes: Arc<Vec<u8>>,
    preview: Arc<Image>,
}

/// The pending question panel's answers for one request.
#[derive(Default)]
struct QuestionState {
    request_id: Option<ApprovalRequestId>,
    index: usize,
    answers: BTreeMap<String, DraftAnswer>,
}

/// Latest `projects.searchEntries` for the `@` menu.
#[derive(Default)]
struct PathSearch {
    query: Option<String>,
    entries: Vec<ProjectEntry>,
    loading: bool,
    task: Option<Task<()>>,
}

/// The composer for one thread or draft. See the module docs for how a host embeds it.
pub struct Composer {
    environment: Entity<Environment>,
    target: ComposerTarget,
    editor: Entity<TextareaState>,
    focus_handle: FocusHandle,
    thread: Option<Arc<ThreadState>>,
    pending: PendingRequests,
    context_window: Option<ContextWindow>,
    trigger: Option<Trigger>,
    /// Highlighted menu row and the search it belongs to.
    highlight: Option<(String, (TriggerKind, String))>,
    path_search: PathSearch,
    model_picker: Option<Entity<ModelPicker>>,
    traits_open: bool,
    images: Vec<ComposerImage>,
    /// Text of live terminal contexts by id; contexts restored from disk have none.
    terminal_texts: HashMap<String, String>,
    sending: bool,
    responding: HashSet<ApprovalRequestId>,
    question: QuestionState,
    plan_panel_open: bool,
    dragging: bool,
    /// Ignore editor change events while the composer itself rewrites the text.
    applying: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Focusable for Composer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Composer {
    /// A composer for `target` in `environment`, restored from the target's saved draft.
    pub fn new(
        environment: Entity<Environment>,
        target: ComposerTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = editor::new_editor(window, cx);
        let drafts = DraftStore::global(cx);
        let app_state = AppState::global(cx);
        let subscriptions = vec![
            cx.subscribe_in(&editor, window, Self::on_editor_event),
            cx.observe(&environment, |this, _, cx| {
                this.finish_promotion(cx);
                cx.notify();
            }),
            cx.observe(&drafts, |_, _, cx| cx.notify()),
            cx.observe(&app_state, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &app_state,
                window,
                |this, _, event: &AppEvent, window, cx| {
                    if let AppEvent::Command(command) = event {
                        this.on_command(command, window, cx);
                    }
                },
            ),
        ];
        let mut composer = Self {
            environment,
            target,
            editor,
            focus_handle: cx.focus_handle(),
            thread: None,
            pending: PendingRequests::default(),
            context_window: None,
            trigger: None,
            highlight: None,
            path_search: PathSearch::default(),
            model_picker: None,
            traits_open: false,
            images: Vec::new(),
            terminal_texts: HashMap::new(),
            sending: false,
            responding: HashSet::new(),
            question: QuestionState::default(),
            plan_panel_open: false,
            dragging: false,
            applying: false,
            _subscriptions: subscriptions,
        };
        composer.restore_draft(window, cx);
        composer
    }

    /// The thread or draft this composer edits.
    pub fn target(&self) -> &ComposerTarget {
        &self.target
    }

    /// Feeds the thread detail the host follows (session, activities, plans). Pending approvals
    /// and questions, the context meter, and the plan toggle derive from it.
    pub fn set_thread_state(&mut self, state: Arc<ThreadState>, cx: &mut Context<Self>) {
        if self
            .thread
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &state))
        {
            return;
        }
        let activities = state
            .thread
            .as_ref()
            .map(|thread| thread.activities.as_slice())
            .unwrap_or_default();
        self.pending = t3_client::pending_requests(activities);
        self.context_window = pending::latest_context_window(activities);
        self.responding.retain(|id| {
            self.pending.approvals.iter().any(|a| &a.request_id == id)
                || self.pending.user_inputs.iter().any(|q| &q.request_id == id)
        });
        let active = self
            .pending
            .user_inputs
            .first()
            .map(|input| &input.request_id);
        if self.question.request_id.as_ref() != active {
            self.question = QuestionState {
                request_id: active.cloned(),
                ..Default::default()
            };
        }
        self.thread = Some(state);
        cx.notify();
    }

    /// Whether the right panel shows the plan (the footer toggle's pressed state).
    pub fn set_plan_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.plan_panel_open != open {
            self.plan_panel_open = open;
            cx.notify();
        }
    }

    /// Focuses the editor with the caret at the end.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| {
            let end = state.value().len();
            state.set_selected_range(end..end, cx);
            state.focus(window, cx);
        });
    }

    /// Type-to-focus (`chat.md` 2.4): appends `text` and focuses. Refused while connecting, while
    /// an approval or question is pending, or while the environment is unavailable.
    pub fn type_to_focus(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connected(cx) || !self.pending.is_empty() {
            return false;
        }
        self.focus(window, cx);
        self.editor
            .update(cx, |state, cx| state.insert(text.to_owned(), window, cx));
        true
    }

    /// Adds a terminal selection as a chip at the caret (the terminal's "Add to chat").
    pub fn add_terminal_context(
        &mut self,
        context: t3_logic::composer::draft::TerminalContextMeta,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = self.target.key();
        let label = context.label();
        let kind = chips::ChipKind::Terminal {
            context_id: context.id.clone(),
            expired: false,
        };
        self.terminal_texts.insert(context.id.clone(), text);
        DraftStore::global(cx).update(cx, |drafts, cx| {
            drafts.update_draft(&key, |draft| draft.terminal_contexts.push(context), cx)
        });
        let placeholder = prompt::TERMINAL_CONTEXT_PLACEHOLDER.to_string();
        self.editor.update(cx, |state, cx| {
            let cursor = state.cursor();
            let _ = state.replace_range_with_token(
                cursor..cursor,
                kind.token(&placeholder, &label),
                window,
                cx,
            );
        });
    }

    /// Replaces the prompt (chips included) and puts the caret at the end, as if typed.
    pub fn set_prompt(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| {
            state.set_value(text.to_owned(), window, cx);
            let end = text.len();
            state.set_selected_range(end..end, cx);
        });
        self.on_prompt_changed(window, cx);
    }

    /// Attaches an image (paste, drop, or a host's own source). Limits apply.
    pub fn add_image(
        &mut self,
        name: impl Into<String>,
        mime_type: impl Into<String>,
        bytes: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        self.attach_image(name.into(), mime_type.into(), bytes, cx);
    }

    /// Opens the model picker (also `/model` and ⇧⌘M).
    pub fn show_model_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_picker.is_none() {
            self.open_model_picker(window, cx);
        }
    }

    /// Opens the traits menu.
    pub fn show_traits(&mut self, cx: &mut Context<Self>) {
        self.traits_open = true;
        cx.notify();
    }

    /// Shows `entries` as the `@` results for the current query without a server (scenes).
    #[doc(hidden)]
    pub fn preview_path_results(&mut self, entries: Vec<ProjectEntry>, cx: &mut Context<Self>) {
        self.path_search.task = None;
        self.path_search.loading = false;
        self.path_search.entries = entries;
        cx.notify();
    }

    // -----------------------------------------------------------------------------------------
    // State derived for render

    fn connected(&self, cx: &App) -> bool {
        self.environment.read(cx).status().is_connected()
    }

    fn shell_thread(
        &self,
        cx: &App,
    ) -> Option<Arc<t3_protocol::orchestration::OrchestrationThreadShell>> {
        match &self.target {
            ComposerTarget::Thread(thread) => {
                self.environment.read(cx).thread(&thread.thread_id).cloned()
            }
            ComposerTarget::Draft(_) => None,
        }
    }

    /// The thread's session status, from the detail when the host provided it.
    fn session_status(&self, cx: &App) -> Option<SessionStatus> {
        self.thread
            .as_ref()
            .and_then(|state| state.thread.as_ref())
            .and_then(|thread| thread.session.as_ref())
            .map(|session| session.status.clone())
            .or_else(|| {
                self.shell_thread(cx)
                    .and_then(|thread| thread.session.as_ref().map(|s| s.status.clone()))
            })
    }

    fn is_running(&self, cx: &App) -> bool {
        self.session_status(cx) == Some(SessionStatus::Running)
    }

    fn is_connecting(&self, cx: &App) -> bool {
        self.session_status(cx) == Some(SessionStatus::Starting)
    }

    fn project_id(&self, cx: &App) -> Option<t3_protocol::ProjectId> {
        match &self.target {
            ComposerTarget::Thread(_) => self
                .shell_thread(cx)
                .map(|thread| thread.project_id.clone()),
            ComposerTarget::Draft(id) => DraftStore::global_ref(cx).and_then(|drafts| {
                drafts
                    .read(cx)
                    .draft_thread(id)
                    .map(|d| d.project_id.clone())
            }),
        }
    }

    /// The project's workspace root (the `cwd` of path search).
    fn workspace_root(&self, cx: &App) -> Option<String> {
        let project_id = self.project_id(cx)?;
        let environment = self.environment.read(cx);
        let root = environment.project(&project_id)?.workspace_root.clone();
        let worktree = match &self.target {
            ComposerTarget::Thread(_) => {
                self.shell_thread(cx).and_then(|t| t.worktree_path.clone())
            }
            ComposerTarget::Draft(id) => DraftStore::global_ref(cx).and_then(|drafts| {
                drafts
                    .read(cx)
                    .draft_thread(id)
                    .and_then(|d| d.worktree_path.clone())
            }),
        };
        Some(worktree.unwrap_or(root))
    }

    /// Resolves the composer's instance and model (`chat.md` 4.13).
    fn resolved_model(&self, cx: &App) -> Option<(Arc<ServerConfig>, ResolvedModel)> {
        let config = self.environment.read(cx).config()?.clone();
        let drafts = DraftStore::global_ref(cx)?.read(cx);
        let draft = drafts.draft(&self.target.key());
        let draft_selections: Vec<ModelSelection> = draft
            .map(|draft| {
                draft
                    .model_selection_by_provider
                    .values()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let shell_thread = self.shell_thread(cx);
        let session_instance = self
            .thread
            .as_ref()
            .and_then(|state| state.thread.as_ref())
            .and_then(|thread| thread.session.as_ref())
            .and_then(|session| session.provider_instance_id.clone())
            .or_else(|| {
                shell_thread
                    .as_ref()
                    .and_then(|thread| thread.session.as_ref())
                    .and_then(|session| session.provider_instance_id.clone())
            });
        let project_selection = self.project_id(cx).and_then(|id| {
            self.environment
                .read(cx)
                .project(&id)
                .and_then(|project| project.default_model_selection.clone())
        });
        let (sticky_selections, sticky_instance) = drafts.sticky();
        let context = ModelContext {
            draft_active_instance: draft.and_then(|draft| draft.active_provider.as_ref()),
            draft_selections: &draft_selections,
            session_instance: session_instance.as_ref(),
            thread_selection: shell_thread.as_ref().map(|thread| &thread.model_selection),
            project_selection: project_selection.as_ref(),
            sticky_selections,
            sticky_instance,
        };
        let resolved = providers::resolve_model_selection(&config.providers, &context);
        Some((config, resolved))
    }

    /// The prompt as the draft holds it.
    fn prompt(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    fn approval_active(&self) -> bool {
        !self.pending.approvals.is_empty()
    }

    fn question_shapes(&self) -> Vec<(String, bool)> {
        self.pending
            .user_inputs
            .first()
            .map(|input| {
                input
                    .questions
                    .iter()
                    .map(|question| (question.id.clone(), question.multi_select))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn question_progress(&self) -> Option<pending::Progress> {
        let shapes = self.question_shapes();
        if shapes.is_empty() {
            return None;
        }
        let shapes: Vec<(&str, bool)> = shapes.iter().map(|(id, m)| (id.as_str(), *m)).collect();
        Some(pending::progress(
            &shapes,
            &self.question.answers,
            self.question.index,
        ))
    }

    fn live_terminal_contexts(&self, cx: &App) -> usize {
        DraftStore::global_ref(cx)
            .and_then(|drafts| {
                drafts.read(cx).draft(&self.target.key()).map(|draft| {
                    draft
                        .terminal_contexts
                        .iter()
                        .filter(|context| self.terminal_texts.contains_key(&context.id))
                        .count()
                })
            })
            .unwrap_or_default()
    }

    fn has_sendable_content(&self, cx: &App) -> bool {
        send::has_sendable_content(
            &self.prompt(cx),
            self.images.len(),
            self.live_terminal_contexts(cx),
        )
    }

    // -----------------------------------------------------------------------------------------
    // Draft persistence

    fn restore_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.target.key();
        let Some(draft) = DraftStore::global(cx).read(cx).draft(&key).cloned() else {
            return;
        };
        let skills = self.provider_skills(cx);
        let content = editor::content_for_prompt(
            &draft.prompt,
            &draft.terminal_contexts,
            &|id| self.terminal_texts.contains_key(id),
            &skills,
        );
        self.images = draft
            .attachments
            .iter()
            .filter_map(|image| image_from_data_url(image.clone()))
            .collect();
        self.applying = true;
        self.editor
            .update(cx, |state, cx| state.set_value(content, window, cx));
        self.applying = false;
    }

    fn save_prompt(&self, cx: &mut Context<Self>) {
        let key = self.target.key();
        let text = self.prompt(cx);
        let terminal_ids: Vec<String> = self
            .editor
            .read(cx)
            .tokens()
            .iter()
            .filter_map(
                |span| match chips::ChipKind::from_token_id(span.token().id()) {
                    Some(chips::ChipKind::Terminal { context_id, .. }) => Some(context_id),
                    _ => None,
                },
            )
            .collect();
        DraftStore::global(cx).update(cx, |drafts, cx| {
            drafts.update_draft(
                &key,
                |draft| {
                    draft.prompt = text;
                    // A deleted terminal chip drops its context.
                    draft
                        .terminal_contexts
                        .retain(|context| terminal_ids.contains(&context.id));
                },
                cx,
            )
        });
    }

    fn save_images(&self, cx: &mut Context<Self>) {
        let key = self.target.key();
        let images: Vec<DraftImage> = self
            .images
            .iter()
            .map(|image| image.draft.clone())
            .collect();
        DraftStore::global(cx).update(cx, |drafts, cx| {
            drafts.update_draft(&key, |draft| draft.attachments = images, cx)
        });
    }

    fn provider_skills(&self, cx: &App) -> Vec<t3_protocol::server::ProviderSkill> {
        self.resolved_model(cx)
            .and_then(|(config, resolved)| {
                resolved
                    .provider_index
                    .and_then(|index| config.providers.get(index))
                    .map(|provider| provider.skills.clone())
            })
            .unwrap_or_default()
    }

    fn selected_provider<'a>(
        config: &'a ServerConfig,
        resolved: &ResolvedModel,
    ) -> Option<&'a ServerProvider> {
        resolved
            .provider_index
            .and_then(|index| config.providers.get(index))
    }

    // -----------------------------------------------------------------------------------------
    // Editor events and keys

    fn on_editor_event(
        &mut self,
        _: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change if !self.applying => self.on_prompt_changed(window, cx),
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
            _ => {}
        }
    }

    fn on_prompt_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let skills = self.provider_skills(cx);
        self.applying = true;
        editor::sync_chips(&self.editor, &skills, window, cx);
        self.applying = false;

        let text = self.prompt(cx);
        if let Some(question) = self.active_question_id() {
            let answer = self.question.answers.entry(question).or_default();
            pending::set_custom_answer(answer, &text);
        } else if !self.approval_active() {
            self.save_prompt(cx);
        }
        self.update_trigger(cx);
        cx.notify();
    }

    fn active_question_id(&self) -> Option<String> {
        let input = self.pending.user_inputs.first()?;
        let index = self
            .question
            .index
            .min(input.questions.len().saturating_sub(1));
        input
            .questions
            .get(index)
            .map(|question| question.id.clone())
    }

    fn update_trigger(&mut self, cx: &mut Context<Self>) {
        let state = self.editor.read(cx);
        let trigger = if editor::caret_after_chip(state) {
            None
        } else {
            prompt::detect_trigger(&state.value(), state.cursor())
        };
        let path_query = trigger
            .as_ref()
            .filter(|trigger| trigger.kind == TriggerKind::Path)
            .map(|trigger| trigger.query.clone());
        self.trigger = trigger;
        if path_query != self.path_search.query {
            self.search_paths(path_query, cx);
        }
    }

    /// Debounced `projects.searchEntries` for the `@` query (120ms, 80 results). Empty queries
    /// search nothing.
    fn search_paths(&mut self, query: Option<String>, cx: &mut Context<Self>) {
        self.path_search.query = query.clone();
        let Some(query) = query.filter(|query| !query.is_empty()) else {
            self.path_search.entries.clear();
            self.path_search.loading = false;
            self.path_search.task = None;
            return;
        };
        let (Some(client), Some(cwd)) = (
            self.environment.read(cx).client().cloned(),
            self.workspace_root(cx),
        ) else {
            return;
        };
        self.path_search.loading = true;
        let delay = cx
            .background_executor()
            .timer(Duration::from_millis(menu::PATH_SEARCH_DEBOUNCE_MS));
        self.path_search.task = Some(cx.spawn(async move |this, cx| {
            delay.await;
            let input = ProjectSearchEntriesInput {
                cwd,
                query: query.clone(),
                limit: menu::PATH_SEARCH_LIMIT,
                kind: None,
                image_only: None,
            };
            let result = cx
                .background_spawn(
                    async move { client.request::<ProjectsSearchEntries>(&input).await },
                )
                .await;
            this.update(cx, |this, cx| {
                if this.path_search.query.as_deref() != Some(query.as_str()) {
                    return;
                }
                this.path_search.loading = false;
                this.path_search.entries = result.map(|r| r.entries).unwrap_or_default();
                cx.notify();
            })
            .ok();
        }));
    }

    /// Rows of the open command menu.
    fn menu_items(&self, cx: &App) -> Vec<MenuItem> {
        let Some(trigger) = &self.trigger else {
            return Vec::new();
        };
        let resolved = self.resolved_model(cx);
        let provider = resolved
            .as_ref()
            .and_then(|(config, resolved)| Self::selected_provider(config, resolved));
        menu::menu_items(trigger, provider, &self.path_search.entries)
    }

    fn menu_open(&self) -> bool {
        self.trigger.is_some() && !self.approval_active()
    }

    fn active_menu_item(&self, items: &[MenuItem]) -> Option<MenuItem> {
        let current = self.trigger.as_ref().map(menu::search_key);
        let (highlighted, key) = match &self.highlight {
            Some((id, key)) => (Some(id.as_str()), Some(key.clone())),
            None => (None, None),
        };
        menu::active_item(items, highlighted, key == current).cloned()
    }

    fn set_highlight(&mut self, item_id: Option<String>, cx: &mut Context<Self>) {
        let key = self.trigger.as_ref().map(menu::search_key);
        self.highlight = item_id.zip(key);
        cx.notify();
    }

    /// ↑/↓ in the editor: move the menu highlight when the menu is open.
    fn on_move(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        if !self.menu_open() {
            return false;
        }
        let items = self.menu_items(cx);
        let active = self.active_menu_item(&items).map(|item| item.id);
        let next = menu::nudge(&items, active.as_deref(), down).map(|item| item.id.clone());
        self.set_highlight(next, cx);
        true
    }

    /// Enter or Tab with the menu open selects the active row.
    fn select_active_menu_item(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.menu_open() {
            return false;
        }
        let items = self.menu_items(cx);
        if let Some(item) = self.active_menu_item(&items) {
            self.select_menu_item(&item, window, cx);
        }
        true
    }

    fn select_menu_item(&mut self, item: &MenuItem, window: &mut Window, cx: &mut Context<Self>) {
        let Some(trigger) = self.trigger.clone() else {
            return;
        };
        let skills = self.provider_skills(cx);
        self.highlight = None;
        match (&item.action, item.replacement()) {
            (MenuAction::OpenModelPicker, _) => {
                editor::replace_text(&self.editor, trigger.range, "", window, cx);
                self.open_model_picker(window, cx);
            }
            (MenuAction::Path { path, .. }, Some(Replacement::Chip { .. })) => editor::insert_chip(
                &self.editor,
                trigger.range,
                InlineTokenKind::Mention { path: path.clone() },
                &skills,
                window,
                cx,
            ),
            (MenuAction::Skill { name }, Some(Replacement::Chip { .. })) => editor::insert_chip(
                &self.editor,
                trigger.range,
                InlineTokenKind::Skill { name: name.clone() },
                &skills,
                window,
                cx,
            ),
            (_, Some(Replacement::Text(text))) => {
                let text_value = self.prompt(cx);
                let end = prompt::replacement_end(&text_value, trigger.range.end, &text);
                editor::replace_text(&self.editor, trigger.range.start..end, &text, window, cx);
            }
            _ => {}
        }
        self.on_prompt_changed(window, cx);
    }

    /// Plain Enter: select a menu row, advance a question, or send.
    fn on_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.select_active_menu_item(window, cx) {
            return;
        }
        if !self.pending.user_inputs.is_empty() {
            self.advance_question(cx);
            return;
        }
        self.send(window, cx);
    }

    fn on_command(
        &mut self,
        command: &t3_logic::keybindings::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use t3_logic::keybindings::Command;
        match command {
            Command::ModelPickerToggle => {
                if self.model_picker.is_some() {
                    self.close_model_picker(window, cx);
                } else {
                    self.open_model_picker(window, cx);
                }
            }
            Command::ModelPickerJump(index) => {
                if let Some(picker) = &self.model_picker {
                    picker.update(cx, |picker, cx| picker.jump(*index as usize - 1, cx));
                }
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------------------------
    // Model picker and traits

    fn open_model_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((config, resolved)) = self.resolved_model(cx) else {
            return;
        };
        let favorites = AppState::global(cx).read(cx).settings().favorites.clone();
        let picker = cx.new(|cx| {
            ModelPicker::new(
                config,
                resolved.instance_id.clone(),
                resolved.model.clone(),
                favorites,
                window,
                cx,
            )
        });
        self._subscriptions.push(cx.subscribe_in(
            &picker,
            window,
            |this, _, event: &ModelPickerEvent, window, cx| match event {
                ModelPickerEvent::Selected(selection) => {
                    this.pick_model(selection.clone(), cx);
                    this.close_model_picker(window, cx);
                }
                ModelPickerEvent::Dismissed => this.close_model_picker(window, cx),
            },
        ));
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.model_picker = Some(picker);
        ShortcutScope::update(cx, |scope| scope.model_picker_open = true);
        cx.notify();
    }

    fn close_model_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_picker.take().is_some() {
            ShortcutScope::update(cx, |scope| scope.model_picker_open = false);
            self.editor.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
        }
    }

    /// Saves a model pick in the draft and as the sticky choice. Nothing is dispatched until
    /// the next send.
    fn pick_model(&mut self, selection: ModelSelection, cx: &mut Context<Self>) {
        let key = self.target.key();
        let drafts = DraftStore::global(cx);
        drafts.update(cx, |drafts, cx| {
            drafts.update_draft(
                &key,
                |draft| {
                    draft.active_provider = Some(selection.instance_id.clone());
                    let entry = draft
                        .model_selection_by_provider
                        .entry(selection.instance_id.to_string())
                        .or_insert_with(|| selection.clone());
                    if entry.model != selection.model {
                        *entry = selection.clone();
                    }
                },
                cx,
            );
            drafts.set_sticky(selection, cx);
        });
    }

    /// Writes new option picks for the selected model.
    fn set_model_options(
        &mut self,
        options: Vec<t3_protocol::orchestration::ProviderOptionSelection>,
        cx: &mut Context<Self>,
    ) {
        let Some((_, resolved)) = self.resolved_model(cx) else {
            return;
        };
        let selection = ModelSelection {
            instance_id: resolved.instance_id,
            model: resolved.model,
            options,
        };
        let key = self.target.key();
        DraftStore::global(cx).update(cx, |drafts, cx| {
            drafts.update_draft(
                &key,
                |draft| {
                    draft
                        .model_selection_by_provider
                        .insert(selection.instance_id.to_string(), selection.clone());
                },
                cx,
            );
            drafts.set_sticky(selection, cx);
        });
    }

    // -----------------------------------------------------------------------------------------
    // Approvals and questions

    fn respond_to_approval(
        &mut self,
        request_id: ApprovalRequestId,
        decision: ApprovalDecision,
        cx: &mut Context<Self>,
    ) {
        let ComposerTarget::Thread(thread) = &self.target else {
            return;
        };
        self.responding.insert(request_id.clone());
        let task = self.environment.read(cx).dispatch(
            commands::respond_to_approval(thread.thread_id.clone(), request_id.clone(), decision),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if let Err(error) = result {
                    this.responding.remove(&request_id);
                    cx.emit(ComposerEvent::Error(error.to_string().into()));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Digits 1-9 pick question options while focus is outside the editor.
    fn on_option_digit(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if self.pending.user_inputs.is_empty()
            || modifiers.platform
            || modifiers.control
            || modifiers.alt
            || self.editor.read(cx).focus_handle(cx).is_focused(window)
        {
            return;
        }
        let Some(digit) = event
            .keystroke
            .key
            .parse::<usize>()
            .ok()
            .filter(|digit| (1..=9).contains(digit))
        else {
            return;
        };
        let input = &self.pending.user_inputs[0];
        if self.responding.contains(&input.request_id) {
            return;
        }
        let index = self
            .question
            .index
            .min(input.questions.len().saturating_sub(1));
        let Some(option) = input
            .questions
            .get(index)
            .and_then(|question| question.options.get(digit - 1))
        else {
            return;
        };
        let label = option.label.clone();
        cx.stop_propagation();
        self.toggle_question_option(label, cx);
    }

    fn toggle_question_option(&mut self, label: String, cx: &mut Context<Self>) {
        let Some(input) = self.pending.user_inputs.first() else {
            return;
        };
        let index = self
            .question
            .index
            .min(input.questions.len().saturating_sub(1));
        let Some(question) = input.questions.get(index) else {
            return;
        };
        let (id, multi) = (question.id.clone(), question.multi_select);
        let answer = self.question.answers.entry(id).or_default();
        pending::toggle_option(multi, answer, &label);
        cx.notify();
        if !multi {
            // Single-select advances after 200ms, like the web.
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                this.update(cx, |this, cx| this.advance_question(cx)).ok();
            })
            .detach();
        }
    }

    fn previous_question(&mut self, cx: &mut Context<Self>) {
        self.question.index = self.question.index.saturating_sub(1);
        cx.notify();
    }

    /// Next question, or submits all answers on the last one.
    fn advance_question(&mut self, cx: &mut Context<Self>) {
        let Some(progress) = self.question_progress() else {
            return;
        };
        let Some(input) = self.pending.user_inputs.first().cloned() else {
            return;
        };
        if self.responding.contains(&input.request_id) {
            return;
        }
        if !progress.is_last {
            if progress.can_advance {
                self.question.index = progress.question_index + 1;
                cx.notify();
            }
            return;
        }
        let shapes = self.question_shapes();
        let shapes: Vec<(&str, bool)> = shapes.iter().map(|(id, m)| (id.as_str(), *m)).collect();
        let Some(answers) = pending::build_answers(&shapes, &self.question.answers) else {
            return;
        };
        let ComposerTarget::Thread(thread) = &self.target else {
            return;
        };
        self.responding.insert(input.request_id.clone());
        let task = self.environment.read(cx).dispatch(
            commands::respond_to_user_input(
                thread.thread_id.clone(),
                input.request_id.clone(),
                answers,
            ),
            cx,
        );
        let request_id = input.request_id;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if let Err(error) = result {
                    this.responding.remove(&request_id);
                    cx.emit(ComposerEvent::Error(error.to_string().into()));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    // -----------------------------------------------------------------------------------------
    // Attachments

    /// Pasted clipboard images become attachments; text pastes normally.
    fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let mut handled = false;
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    handled = true;
                    let extension = image.format.extension();
                    self.attach_image(
                        format!("pasted-image.{extension}"),
                        image.format.mime_type().to_owned(),
                        image.bytes.clone(),
                        cx,
                    );
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    handled = true;
                    self.attach_paths(paths, cx);
                }
                ClipboardEntry::String(_) => {}
            }
        }
        if handled {
            cx.stop_propagation();
        }
    }

    fn attach_paths(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        for path in paths.paths() {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mime = send::image_mime_for_path(&name).unwrap_or("application/octet-stream");
            let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
            if let Err(error) = send::check_attachment(&name, mime, size, self.images.len()) {
                cx.emit(ComposerEvent::Error(error.to_string().into()));
                continue;
            }
            match std::fs::read(path) {
                Ok(bytes) => self.attach_image(name, mime.to_owned(), bytes, cx),
                Err(error) => cx.emit(ComposerEvent::Error(format!("{name}: {error}").into())),
            }
        }
    }

    fn attach_image(
        &mut self,
        name: String,
        mime_type: String,
        bytes: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        if !self.pending.user_inputs.is_empty() {
            crate::toast::show(
                crate::toast::Toast::info("Attach images after answering plan questions."),
                cx,
            );
            return;
        }
        if let Err(error) =
            send::check_attachment(&name, &mime_type, bytes.len() as u64, self.images.len())
        {
            cx.emit(ComposerEvent::Error(error.to_string().into()));
            return;
        }
        let data_url = format!("data:{mime_type};base64,{}", base64_encode(&bytes));
        let Some(image) = image_from_data_url(DraftImage {
            id: uuid_like(),
            name,
            mime_type,
            size_bytes: bytes.len() as u64,
            data_url,
        }) else {
            return;
        };
        self.images.push(image);
        self.save_images(cx);
        cx.notify();
    }

    fn remove_image(&mut self, id: &str, cx: &mut Context<Self>) {
        self.images.retain(|image| image.draft.id != id);
        self.save_images(cx);
        cx.notify();
    }

    // -----------------------------------------------------------------------------------------
    // Send

    fn interrupt(&mut self, cx: &mut Context<Self>) {
        let ComposerTarget::Thread(thread) = &self.target else {
            return;
        };
        let turn = self
            .thread
            .as_ref()
            .and_then(|state| state.thread.as_ref())
            .and_then(|thread| thread.session.as_ref())
            .and_then(|session| session.active_turn_id.clone());
        self.environment
            .read(cx)
            .dispatch(commands::interrupt_turn(thread.thread_id.clone(), turn), cx)
            .detach_and_log_err(cx);
    }

    /// The send pipeline (`chat.md` 4.9). While a turn runs, this still dispatches
    /// `thread.turn.start`: the server treats it as a steer.
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sending || !self.connected(cx) || self.is_connecting(cx) {
            return;
        }
        if !self.has_sendable_content(cx) {
            return;
        }
        let Some((config, resolved)) = self.resolved_model(cx) else {
            return;
        };
        let prompt_text = self.prompt(cx);
        let images = self.images.clone();
        let key = self.target.key();
        let draft_before = DraftStore::global(cx).read(cx).draft(&key).cloned();

        let model = Self::selected_provider(&config, &resolved)
            .and_then(|provider| providers::resolve_model(&provider.models, &resolved.model));
        let options = providers::selections_from_descriptors(&providers::option_descriptors(
            model,
            &resolved.options,
        ));
        let model_selection = ModelSelection {
            instance_id: resolved.instance_id.clone(),
            model: resolved.model.clone(),
            options,
        };
        let text = send::outgoing_text(&self.materialized_prompt(&prompt_text, cx), images.len());
        let title = send::title_seed(
            &prompt_text,
            images.first().map(|image| image.draft.name.as_str()),
        );

        let environment_id = self.environment.read(cx).id().clone();
        let (thread_id, bootstrap, extra_commands) = match &self.target {
            ComposerTarget::Thread(thread) => {
                let shell = self.shell_thread(cx);
                let mut extra = Vec::new();
                if let Some(shell) = &shell {
                    let first_message = shell.latest_turn.is_none();
                    let mut patch = ThreadMetaPatch::default();
                    if first_message {
                        patch.title = Some(title.clone());
                    }
                    if shell.model_selection != model_selection {
                        patch.model_selection = Some(model_selection.clone());
                    }
                    if patch.title.is_some() || patch.model_selection.is_some() {
                        extra.push(commands::update_thread(thread.thread_id.clone(), patch));
                    }
                    if shell.runtime_mode != RuntimeMode::FullAccess {
                        extra.push(commands::set_runtime_mode(
                            thread.thread_id.clone(),
                            RuntimeMode::FullAccess,
                        ));
                    }
                    if shell
                        .interaction_mode
                        .as_ref()
                        .is_some_and(|mode| *mode != InteractionMode::Default)
                    {
                        extra.push(commands::set_interaction_mode(
                            thread.thread_id.clone(),
                            InteractionMode::Default,
                        ));
                    }
                }
                (thread.thread_id.clone(), None, extra)
            }
            ComposerTarget::Draft(id) => {
                let Some(draft) = DraftStore::global(cx).read(cx).draft_thread(id).cloned() else {
                    return;
                };
                let project_root = self
                    .environment
                    .read(cx)
                    .project(&draft.project_id)
                    .map(|project| project.workspace_root.clone());
                let prepare_worktree = match (draft.env_mode, draft.worktree_path.as_ref()) {
                    (DraftEnvMode::Worktree, None) => {
                        let Some(base_branch) = draft.branch.clone() else {
                            cx.emit(ComposerEvent::Error(
                                "Select a base branch before sending in New worktree mode.".into(),
                            ));
                            return;
                        };
                        project_root.map(|project_cwd| BootstrapPrepareWorktree {
                            project_cwd,
                            base_branch,
                            branch: Some(format!(
                                "t3/{}",
                                &draft.thread_id.as_str()[..8.min(draft.thread_id.as_str().len())]
                            )),
                            start_from_origin: draft.start_from_origin.then_some(true),
                            require_worktree: None,
                        })
                    }
                    _ => None,
                };
                let bootstrap = TurnStartBootstrap {
                    create_thread: Some(BootstrapCreateThread {
                        project_id: draft.project_id.clone(),
                        title: title.clone(),
                        model_selection: model_selection.clone(),
                        runtime_mode: RuntimeMode::FullAccess,
                        interaction_mode: InteractionMode::Default,
                        branch: draft.branch.clone(),
                        worktree_path: draft.worktree_path.clone(),
                        created_at: commands::now(),
                    }),
                    run_setup_script: prepare_worktree.is_some().then_some(true),
                    prepare_worktree,
                };
                (draft.thread_id.clone(), Some(bootstrap), Vec::new())
            }
        };

        let mut turn = commands::turn_start(
            thread_id.clone(),
            text.clone(),
            RuntimeMode::FullAccess,
            InteractionMode::Default,
        );
        turn.model_selection = Some(model_selection);
        turn.title_seed = Some(title);
        turn.bootstrap = bootstrap;
        let message_id = turn.message.message_id.clone();
        let thread_ref = ThreadRef::new(environment_id, thread_id);
        let uploads = config.environment.capabilities.attachment_uploads;

        // Clear the composer right away; restore it if the dispatch fails.
        self.sending = true;
        self.images.clear();
        self.applying = true;
        self.editor
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.applying = false;
        self.trigger = None;
        DraftStore::global(cx).update(cx, |drafts, cx| {
            drafts.update_draft(&key, |draft| draft.clear_content(), cx)
        });
        cx.emit(ComposerEvent::Sending {
            thread: thread_ref.clone(),
            message_id,
            text,
        });
        cx.notify();

        let client = self.environment.read(cx).client().cloned();
        let target = self.target.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result: anyhow::Result<()> = async {
                let client = client.ok_or_else(|| anyhow::anyhow!("Environment disconnected"))?;
                let mut attachments = Vec::with_capacity(images.len());
                for image in &images {
                    attachments.push(if uploads {
                        let client = client.clone();
                        let (name, mime, bytes) = (
                            image.draft.name.clone(),
                            image.draft.mime_type.clone(),
                            image.bytes.as_ref().clone(),
                        );
                        cx.background_spawn(async move {
                            client
                                .upload_attachment(AttachmentKind::Image, name, mime, bytes)
                                .await
                        })
                        .await?
                    } else {
                        TurnAttachment {
                            kind: AttachmentKind::Image,
                            id: None,
                            name: image.draft.name.clone(),
                            mime_type: image.draft.mime_type.clone(),
                            size_bytes: image.draft.size_bytes,
                            data_url: Some(image.draft.data_url.clone()),
                        }
                    });
                }
                turn.message.attachments = attachments;
                for command in extra_commands {
                    let client = client.clone();
                    cx.background_spawn(async move { client.dispatch(command).await })
                        .await
                        .map_err(|error| anyhow::anyhow!("{error}"))?;
                }
                let client = client.clone();
                cx.background_spawn(async move { client.dispatch(turn.into()).await })
                    .await
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
                Ok(())
            }
            .await;
            this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Ok(()) => {
                        if let ComposerTarget::Draft(id) = &target {
                            DraftStore::global(cx)
                                .update(cx, |drafts, cx| drafts.mark_promoted(id, &thread_ref, cx));
                            cx.emit(ComposerEvent::ThreadStarted(thread_ref.clone()));
                        }
                        this.finish_promotion(cx);
                    }
                    Err(error) => {
                        tracing::warn!("send failed: {error:#}");
                        if this.prompt(cx).is_empty() && this.images.is_empty() {
                            this.images = images;
                            if let Some(draft) = draft_before {
                                let content = editor::content_for_prompt(
                                    &draft.prompt,
                                    &draft.terminal_contexts,
                                    &|id| this.terminal_texts.contains_key(id),
                                    &this.provider_skills(cx),
                                );
                                this.editor
                                    .update(cx, |state, cx| state.set_value(content, window, cx));
                                this.save_prompt(cx);
                                this.save_images(cx);
                            }
                        }
                        cx.emit(ComposerEvent::SendFailed(send::SEND_FAILED.into()));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The prompt with terminal placeholders replaced by their context blocks
    /// (`lib/terminalContext.ts`): `@label` inline, the selected text appended.
    fn materialized_prompt(&self, prompt_text: &str, cx: &App) -> String {
        let contexts = DraftStore::global_ref(cx)
            .and_then(|drafts| {
                drafts
                    .read(cx)
                    .draft(&self.target.key())
                    .map(|d| d.terminal_contexts.clone())
            })
            .unwrap_or_default();
        if contexts.is_empty() {
            return prompt_text.replace(prompt::TERMINAL_CONTEXT_PLACEHOLDER, "");
        }
        let mut inline = String::new();
        let mut blocks = Vec::new();
        let mut contexts = contexts.iter();
        for ch in prompt_text.chars() {
            if ch != prompt::TERMINAL_CONTEXT_PLACEHOLDER {
                inline.push(ch);
                continue;
            }
            let Some(context) = contexts.next() else {
                continue;
            };
            let Some(text) = self.terminal_texts.get(&context.id) else {
                continue;
            };
            let label = format!(
                "@{}:{}",
                context.terminal_label.to_lowercase().replace(' ', "-"),
                if context.line_start == context.line_end {
                    context.line_start.to_string()
                } else {
                    format!("{}-{}", context.line_start, context.line_end)
                }
            );
            inline.push_str(&label);
            blocks.push(format!("- {} {}:\n{}", context.terminal_label, label, text));
        }
        if blocks.is_empty() {
            return inline;
        }
        format!(
            "{}\n\n<terminal_context>\n{}\n</terminal_context>",
            inline.trim_end(),
            blocks.join("\n\n")
        )
    }

    /// After a draft's first send: once the server thread shows up in the shell, swap the route
    /// to it and drop the draft session.
    fn finish_promotion(&mut self, cx: &mut Context<Self>) {
        let ComposerTarget::Draft(id) = &self.target else {
            return;
        };
        let Some(drafts) = DraftStore::global_ref(cx) else {
            return;
        };
        let Some(promoted) = drafts
            .read(cx)
            .draft_thread(id)
            .and_then(|draft| draft.promoted_to.clone())
        else {
            return;
        };
        if self
            .environment
            .read(cx)
            .thread(&promoted.thread_id)
            .is_none()
        {
            return;
        }
        let thread = ThreadRef::new(promoted.environment_id, promoted.thread_id);
        let id = id.clone();
        drafts.update(cx, |drafts, cx| drafts.finalize_promoted(&id, &thread, cx));
        AppState::global(cx).update(cx, |state, cx| {
            if state.route() == &Route::Draft(id.clone()) {
                state.replace_route(Route::Thread(thread), cx);
            }
        });
    }
}

/// Decodes a draft image's data URL into bytes and a preview image.
fn image_from_data_url(draft: DraftImage) -> Option<ComposerImage> {
    let (_, payload) = draft.data_url.split_once(";base64,")?;
    let bytes = base64_decode(payload)?;
    let format = ImageFormat::from_mime_type(&draft.mime_type)?;
    Some(ComposerImage {
        preview: Arc::new(Image::from_bytes(format, bytes.clone())),
        bytes: Arc::new(bytes),
        draft,
    })
}

fn uuid_like() -> String {
    ThreadId::random().to_string()
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' => continue,
            _ => return None,
        };
        buffer = buffer << 6 | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        editor::apply_style(&self.editor, cx);
        let unavailable = !self.connected(cx);
        let focused = self.editor.read(cx).focus_handle(cx).is_focused(window);
        let border = if self.dragging {
            colors.primary_70
        } else if focused {
            colors.ring_45
        } else {
            colors.border
        };
        let has_header = self.approval_active() || !self.pending.user_inputs.is_empty();
        let shadows = if colors.is_dark {
            shadow::COMPOSER_DARK.to_vec()
        } else {
            shadow::COMPOSER_LIGHT.to_vec()
        };
        let disabled = self.is_connecting(cx)
            || self.approval_active()
            || (unavailable && self.pending.user_inputs.is_empty());
        let placeholder: SharedString = if let Some(approval) = self.pending.approvals.first() {
            approval
                .detail
                .clone()
                .unwrap_or_else(|| "Resolve this approval request to continue".into())
                .into()
        } else if !self.pending.user_inputs.is_empty() {
            "Type your own answer, or leave this blank to use the selected option".into()
        } else if unavailable {
            let environment = self.environment.read(cx);
            format!(
                "{}: {}",
                environment.label(),
                environment.status().status_text()
            )
            .into()
        } else if self.thread_started(cx) && self.session_status(cx).is_none() {
            "Ask for follow-up changes or attach images".into()
        } else {
            "Ask anything, @tag files/folders, $use skills, or / for commands".into()
        };
        let prompt_empty = self.editor.read(cx).value().is_empty();
        self.editor
            .update(cx, |state, cx| state.set_disabled(disabled, cx));

        let content = div()
            .relative()
            .px(px(16.))
            .pb(px(8.))
            .pt(if has_header { px(12.) } else { px(16.) })
            .when(self.menu_open(), |this| {
                this.child(self.render_command_menu(window, cx))
            })
            .when(!has_header && !self.images.is_empty(), |this| {
                this.child(self.render_images(cx))
            })
            .child(
                div()
                    .id("composer-editor")
                    .relative()
                    .min_h(EDITOR_MIN_HEIGHT)
                    .text_size(EDITOR_TEXT)
                    .line_height(EDITOR_LINE_HEIGHT)
                    .text_color(colors.foreground)
                    .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                        if this.on_move(false, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                        if this.on_move(true, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|this, _: &IndentInline, window, cx| {
                        if this.select_active_menu_item(window, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|this, action: &Enter, window, cx| {
                        if !action.shift && !action.secondary {
                            cx.stop_propagation();
                            this.on_enter(window, cx);
                        }
                    }))
                    .capture_action(cx.listener(Self::on_paste))
                    .child(Textarea::new(&self.editor).token(chips::render_chip))
                    .when(prompt_empty, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .text_color(style::alpha(colors.muted_foreground, 0.35))
                                .truncate()
                                .child(placeholder),
                        )
                    }),
            );

        let surface = div()
            .id("composer-surface")
            .relative()
            .rounded(px(20.))
            .border_1()
            .border_color(border)
            .bg(if self.dragging {
                colors.accent_45
            } else {
                colors.card
            })
            .shadow(shadows)
            .when(unavailable, |this| this.opacity(0.75))
            .when(has_header, |this| this.child(self.render_header_panel(cx)))
            .child(content)
            .child(self.render_footer(window, cx));

        // The slot is the frame (`rounded-[22px] p-px`); the host centers it in the 768px column.
        div()
            .id("composer-frame")
            .track_focus(&self.focus_handle)
            .relative()
            .w_full()
            .min_w_0()
            .rounded(px(22.))
            .p(px(1.))
            .on_prepaint(self.measure_form(cx))
            .on_key_down(cx.listener(Self::on_option_digit))
            .on_drag_move::<ExternalPaths>(cx.listener(|this, _, _, cx| {
                if !this.dragging {
                    this.dragging = true;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.dragging = false;
                this.attach_paths(paths, cx);
                this.focus(window, cx);
            }))
            .child(surface)
    }
}

impl Composer {
    /// Whether the thread has a turn already (placeholder copy, title seeding).
    fn thread_started(&self, cx: &App) -> bool {
        self.shell_thread(cx)
            .is_some_and(|thread| thread.latest_turn.is_some())
    }

    /// The 64px thumbnail row above the editor (`chat.md` 4.3).
    fn render_images(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .mb(px(12.))
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .children(self.images.iter().map(|image| {
                let id = image.draft.id.clone();
                div()
                    .id(SharedString::from(format!("image-{id}")))
                    .relative()
                    .size(px(64.))
                    .overflow_hidden()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(colors.border_80)
                    .bg(colors.background)
                    .child(
                        img(image.preview.clone())
                            .size_full()
                            .object_fit(gpui_kit::ObjectFit::Cover),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("remove-image-{id}")))
                            .absolute()
                            .top(px(4.))
                            .right(px(4.))
                            .size(px(24.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(colors.background.opacity(0.8))
                            .hover(|style| style.bg(colors.background.opacity(0.9)))
                            .cursor_pointer()
                            .child(
                                Icon::new(IconName::X)
                                    .size(px(14.))
                                    .color(colors.foreground),
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.remove_image(&id, cx)),
                            ),
                    )
            }))
    }
}
