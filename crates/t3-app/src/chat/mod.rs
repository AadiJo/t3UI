//! The chat view of one thread or draft (`ChatView.tsx`, spec `chat.md`): the 52px header,
//! the banners, the message timeline, and the composer overlay.
//!
//! Mounting (the shell does this in `workspace::build_main_view`):
//!
//! ```ignore
//! cx.new(|cx| ChatView::new(ChatTarget::Thread(thread_ref), app_state, window, cx)).into()
//! ```
//!
//! Slots: other modules provide the composer and the header actions (scripts / Open in / Git)
//! by registering builders once at startup with [`register_slot`]; each chat view builds its
//! slot views once, in [`ChatView::new`]. Builders get a [`SlotContext`] with the target and a
//! weak handle to the view.
//!
//! The composer slot is the whole composer stack (attached drawers, card, context strip). The
//! chat view lays it out in the overlay (`composer.md` 1): 8px top padding, the 20px gutter,
//! the 768px chat column, and a 20px spacer under it. It measures the overlay itself and turns
//! the height into the timeline's end inset (`resolveComposerTimelineInset`).
//!
//! Between the composer and the chat view:
//! - composer -> chat: [`ChatView::thread`] (re-read on `cx.observe(&chat, ..)`),
//!   [`ChatView::begin_local_dispatch`] / [`ChatView::end_local_dispatch`] around sends,
//!   [`ChatView::set_thread_error`], [`ChatView::set_composer_resting`],
//!   [`ChatView::set_queued_messages`], [`ChatView::scroll_timeline_page`].
//! - chat -> composer: [`ChatEvent`]s (`cx.subscribe(&chat, ..)`).

mod banners;
mod body;
mod controls;
pub mod fixtures;
mod header;
mod markdown;
mod rows;
mod timeline;

use std::{path::PathBuf, rc::Rc, sync::Arc, time::Duration};

use gpui_kit::{
    AnyView, App, Context, Entity, EventEmitter, Global, IntoElement, ParentElement as _, Pixels,
    Render, Styled as _, Subscription, Task, WeakEntity, Window, div, prelude::FluentBuilder as _,
    px,
};
use t3_client::ThreadState;
use t3_logic::{
    ProjectRef, ThreadRef,
    timeline::{QueuedMessage, SessionPhase, active_work_started_at, is_latest_turn_settled},
};
use t3_protocol::orchestration::{
    ChatAttachment, OrchestrationLatestTurn, OrchestrationMessage, OrchestrationThread,
    SessionStatus,
};
use t3_ui::ActiveColors as _;

use crate::state::{AppState, DraftId, Environment};
use timeline::Timeline;

/// What a chat view shows: a server thread or a not-yet-started draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatTarget {
    Thread(ThreadRef),
    /// A draft; `project` names the project in the header ("New thread · aurora-web").
    Draft {
        id: DraftId,
        project: Option<ProjectRef>,
    },
}

/// A view other modules provide inside the chat view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    /// The composer stack (attached drawers, card, context strip), max 768px wide.
    Composer,
    /// Unused: the composer renders its context strip inside [`Slot::Composer`]. A builder
    /// registered here is drawn under the composer.
    BranchToolbar,
    /// The header's right-side actions (scripts, Open in, Git).
    HeaderActions,
}

/// What the chat view tells the composer slot (`cx.subscribe(&chat, ..)`).
#[derive(Clone, Debug, PartialEq)]
pub enum ChatEvent {
    /// A scroll gesture over the timeline moved away from its end: a wheel gesture of 24px or
    /// more within 120ms, PageUp/Home off the top, or PageDown/End short of the end. The
    /// composer rests if it is eligible (`composer.md` 3).
    CollapseComposer,
    /// The timeline reached its end (40px band) or the "Scroll to end" pill was clicked.
    RestoreComposer,
    /// A queued row's "Send now" (queue entry id).
    SteerQueuedMessage(String),
    /// A queued row's "Cancel and return to the composer" (queue entry id).
    RemoveQueuedMessage(String),
    /// "Edit from here" reverted the thread: append `text` to the draft after a blank line,
    /// re-attach `attachments`, and focus the end.
    RestorePrompt {
        text: String,
        attachments: Vec<ChatAttachment>,
    },
    /// Type-to-focus / paste-to-focus: insert at the end of the composer and focus it.
    InsertText(String),
    /// Files or folders dropped on the chat column.
    FilesDropped(Vec<PathBuf>),
}

/// How much taller the empty expanded composer is than its resting row
/// (`COMPOSER_RESTING_EXPANSION_MIN_PX`).
const COMPOSER_RESTING_EXPANSION: f32 = 94.;

/// The timeline's end inset for the composer overlay (`resolveComposerTimelineInset`): the
/// overlay height while expanded; while resting, the held inset or the resting height plus
/// the empty expansion, whichever is larger, so expanding again never covers rows.
fn resolve_composer_timeline_inset(current: Pixels, overlay: Pixels, resting: bool) -> Pixels {
    if resting {
        current.max(overlay + px(COMPOSER_RESTING_EXPANSION))
    } else {
        overlay
    }
}

/// What a slot builder receives.
#[derive(Clone)]
pub struct SlotContext {
    pub target: ChatTarget,
    pub chat: WeakEntity<ChatView>,
}

type SlotBuilder = Rc<dyn Fn(SlotContext, &mut Window, &mut App) -> AnyView>;

#[derive(Default)]
struct Slots {
    composer: Option<SlotBuilder>,
    branch_toolbar: Option<SlotBuilder>,
    header_actions: Option<SlotBuilder>,
}

impl Global for Slots {}

/// Registers the builder for `slot`; every chat view built afterwards mounts it.
pub fn register_slot(
    slot: Slot,
    builder: impl Fn(SlotContext, &mut Window, &mut App) -> AnyView + 'static,
    cx: &mut App,
) {
    let builder: SlotBuilder = Rc::new(builder);
    let slots = cx.default_global::<Slots>();
    match slot {
        Slot::Composer => slots.composer = Some(builder),
        Slot::BranchToolbar => slots.branch_toolbar = Some(builder),
        Slot::HeaderActions => slots.header_actions = Some(builder),
    }
}

/// A user message sent from this client that the server has not confirmed yet, with the turn
/// and session as they were at send time (`createLocalDispatchSnapshot`).
struct LocalDispatch {
    started_at: String,
    message: Option<Arc<OrchestrationMessage>>,
    turn: Option<OrchestrationLatestTurn>,
    session: Option<(SessionStatus, String)>,
}

impl LocalDispatch {
    /// Whether the server has taken over (`hasServerAcknowledgedLocalDispatch`): a request or
    /// error is pending, a new running turn arrived, or (when not running or connecting) the
    /// turn or session changed at all.
    fn acknowledged(&self, state: &ThreadState, local_error: bool) -> bool {
        let Some(thread) = &state.thread else {
            return false;
        };
        let pending = state.pending_requests();
        let session = thread.session.as_ref();
        if !pending.approvals.is_empty()
            || !pending.user_inputs.is_empty()
            || local_error
            || session.is_some_and(|s| s.last_error.is_some())
        {
            return true;
        }
        let latest = thread.latest_turn.as_ref();
        let turn_key = |turn: Option<&OrchestrationLatestTurn>| {
            turn.map(|t| {
                (
                    t.turn_id.clone(),
                    t.requested_at.clone(),
                    t.started_at.clone(),
                    t.completed_at.clone(),
                )
            })
        };
        let turn_changed = turn_key(self.turn.as_ref()) != turn_key(latest);
        match SessionPhase::of(session) {
            SessionPhase::Running => {
                turn_changed
                    && latest.is_some_and(|turn| {
                        turn.started_at.is_some()
                            && session
                                .and_then(|s| s.active_turn_id.as_ref())
                                .is_none_or(|active| *active == turn.turn_id)
                    })
            }
            // A starting session is only transport-level acknowledgement.
            SessionPhase::Connecting => false,
            _ => {
                turn_changed
                    || self.session != session.map(|s| (s.status.clone(), s.updated_at.clone()))
            }
        }
    }
}

/// The chat view. One per route; switching threads builds a new one.
pub struct ChatView {
    app_state: Entity<AppState>,
    target: ChatTarget,
    environment: Option<Entity<Environment>>,
    thread: Option<Arc<ThreadState>>,
    timeline: Timeline,
    local_dispatch: Option<LocalDispatch>,
    /// A send or revert error from this client; dismissing the banner clears it.
    local_error: Option<String>,
    reverting: bool,
    composer: Option<AnyView>,
    branch_toolbar: Option<AnyView>,
    header_actions: Option<AnyView>,
    /// The composer overlay's measured height (8px top padding through the 20px spacer).
    overlay_height: Pixels,
    /// The end inset the timeline reserves for the overlay (`resolveComposerTimelineInset`).
    timeline_inset: Pixels,
    /// The composer reports its resting (one-line) layout.
    composer_resting: bool,
    /// The composer's queue for this thread, rendered after the live rows.
    queued_messages: Vec<QueuedMessage>,
    /// The wheel gesture in progress over the timeline.
    wheel_gesture: Option<body::WheelGesture>,
    /// The timeline was within the 40px end band at the last render.
    timeline_at_end: bool,
    _tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ChatView {
    pub fn new(
        target: ChatTarget,
        app_state: Entity<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let environment_id = match &target {
            ChatTarget::Thread(thread) => Some(&thread.environment_id),
            ChatTarget::Draft { project, .. } => project.as_ref().map(|p| &p.environment_id),
        };
        let environment = environment_id.and_then(|id| app_state.read(cx).environment(id, cx));
        let mut subscriptions = vec![
            cx.observe(&app_state, |_, _, cx| cx.notify()),
            cx.observe_window_activation(window, |this, window, cx| {
                this.acknowledge_completion(window, cx)
            }),
        ];
        if let Some(environment) = &environment {
            subscriptions.push(cx.observe(environment, |_, _, cx| cx.notify()));
        }

        let mut tasks = Vec::new();
        let mut thread = None;
        if let (ChatTarget::Thread(thread_ref), Some(environment)) = (&target, &environment) {
            match environment
                .read(cx)
                .open_thread(thread_ref.thread_id.clone())
            {
                Some(handle) => {
                    let mut receiver = handle.state();
                    thread = Some(receiver.borrow_and_update().clone());
                    tasks.push(cx.spawn_in(window, async move |this, cx| {
                        // Keeps the subscription open for the view's lifetime.
                        let _handle = handle;
                        while receiver.changed().await.is_ok() {
                            let state = receiver.borrow_and_update().clone();
                            let updated = this.update_in(cx, |this, window, cx| {
                                this.set_thread(state, cx);
                                this.acknowledge_completion(window, cx);
                            });
                            if updated.is_err() {
                                break;
                            }
                        }
                    }));
                }
                None => thread = fixtures::thread(thread_ref, cx),
            }
        }
        tasks.push(Self::tick_working_timer(cx));

        let slot_context = SlotContext {
            target: target.clone(),
            chat: cx.weak_entity(),
        };
        let build = |builder: &Option<SlotBuilder>, window: &mut Window, cx: &mut App| {
            builder
                .clone()
                .map(|builder| builder(slot_context.clone(), window, cx))
        };
        let (composer, branch_toolbar, header_actions) = match cx.try_global::<Slots>() {
            Some(slots) => {
                let (a, b, c) = (
                    slots.composer.clone(),
                    slots.branch_toolbar.clone(),
                    slots.header_actions.clone(),
                );
                (
                    build(&a, window, cx),
                    build(&b, window, cx),
                    build(&c, window, cx),
                )
            }
            None => (None, None, None),
        };

        let mut this = Self {
            app_state,
            target,
            environment,
            thread: None,
            timeline: Timeline::new(cx),
            local_dispatch: None,
            local_error: None,
            reverting: false,
            composer,
            branch_toolbar,
            header_actions,
            overlay_height: px(0.),
            timeline_inset: px(0.),
            composer_resting: false,
            queued_messages: Vec::new(),
            wheel_gesture: None,
            timeline_at_end: true,
            _tasks: tasks,
            _subscriptions: subscriptions,
        };
        if let Some(state) = thread {
            this.set_thread(state, cx);
            this.acknowledge_completion(window, cx);
        }
        this
    }

    /// The open thread's latest state (`None` for drafts and before the first snapshot).
    pub fn thread(&self) -> Option<&Arc<ThreadState>> {
        self.thread.as_ref()
    }

    pub fn target(&self) -> &ChatTarget {
        &self.target
    }

    /// Scrolls the timeline to its first row and stops following the end.
    pub fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.timeline.list.scroll_to(gpui_kit::ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    /// Opens every "Worked for" fold and work group (snapshot scenes of the expanded log).
    pub fn expand_all(&mut self, cx: &mut Context<Self>) {
        loop {
            let mut changed = false;
            for row in self.timeline.rows().to_vec() {
                match &row.kind {
                    t3_logic::timeline::RowKind::TurnFold { turn_id, .. } => {
                        changed |= self.timeline.expanded_turns.insert(turn_id.clone());
                    }
                    t3_logic::timeline::RowKind::WorkToggle { group_id, .. }
                    | t3_logic::timeline::RowKind::ToolStack { group_id, .. } => {
                        changed |= self.timeline.expanded_groups.insert(group_id.clone());
                    }
                    _ => {}
                }
            }
            if !changed {
                break;
            }
            self.timeline.rederive();
        }
        cx.notify();
    }

    /// The composer sent `message`: show it right away, start the "Working" timer, and follow
    /// the end of the timeline. Cleared when the server reports the turn (or on
    /// [`ChatView::end_local_dispatch`]).
    pub fn begin_local_dispatch(
        &mut self,
        message: Option<OrchestrationMessage>,
        cx: &mut Context<Self>,
    ) {
        let now = self.app_state.read(cx).now_millis();
        let thread = self.orchestration_thread();
        self.local_dispatch = Some(LocalDispatch {
            started_at: t3_logic::time::format_timestamp(now).unwrap_or_default(),
            message: message.map(Arc::new),
            turn: thread.and_then(|t| t.latest_turn.clone()),
            session: thread
                .and_then(|t| t.session.as_ref())
                .map(|s| (s.status.clone(), s.updated_at.clone())),
        });
        self.local_error = None;
        self.timeline.follow_end();
        self.refresh_rows(cx);
    }

    /// The send failed or was cancelled; `error` shows in the thread error banner.
    pub fn end_local_dispatch(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.local_dispatch = None;
        self.local_error = error;
        self.refresh_rows(cx);
    }

    fn set_thread(&mut self, state: Arc<ThreadState>, cx: &mut Context<Self>) {
        let local_error = self.local_error.is_some();
        if self
            .local_dispatch
            .as_ref()
            .is_some_and(|dispatch| dispatch.acknowledged(&state, local_error))
        {
            self.local_dispatch = None;
        }
        self.thread = Some(state);
        self.refresh_rows(cx);
    }

    /// Clears the sidebar's "Completed" pill for this thread once the user can see the
    /// finished turn: the window is active and the latest turn has completed
    /// (`ChatView.tsx` completion acknowledgement).
    fn acknowledge_completion(&mut self, window: &Window, cx: &mut Context<Self>) {
        let ChatTarget::Thread(thread_ref) = &self.target else {
            return;
        };
        if !window.is_window_active() {
            return;
        }
        let Some(completed_at) = self
            .orchestration_thread()
            .and_then(|thread| thread.latest_turn.as_ref())
            .and_then(|turn| turn.completed_at.clone())
        else {
            return;
        };
        let key = thread_ref.key();
        let seen =
            self.app_state.read(cx).ui().last_visited_at(&key) == Some(completed_at.as_str());
        if !seen {
            let thread_ref = thread_ref.clone();
            self.app_state.update(cx, |state, cx| {
                state.mark_thread_visited(&thread_ref, &completed_at, cx)
            });
        }
    }

    /// The thread being shown, if loaded.
    fn orchestration_thread(&self) -> Option<&OrchestrationThread> {
        self.thread.as_ref()?.thread.as_ref()
    }

    /// Phase running/connecting, a local send in flight, or a revert in progress
    /// (`ChatView.tsx` `isWorking`).
    fn is_working(&self) -> bool {
        let phase = SessionPhase::of(
            self.orchestration_thread()
                .and_then(|thread| thread.session.as_ref()),
        );
        matches!(phase, SessionPhase::Running | SessionPhase::Connecting)
            || self.local_dispatch.is_some()
            || self.reverting
    }

    /// Work in progress or a latest turn that has not settled: work rows show live status.
    fn turn_in_progress(&self) -> bool {
        let settled = self.orchestration_thread().is_some_and(|thread| {
            is_latest_turn_settled(thread.latest_turn.as_ref(), thread.session.as_ref())
        });
        self.is_working() || !settled
    }

    /// Re-derives the timeline rows from the current thread and UI state.
    fn refresh_rows(&mut self, cx: &mut Context<Self>) {
        let is_working = self.is_working();
        let Some(state) = self.thread.clone().filter(|state| state.thread.is_some()) else {
            cx.notify();
            return;
        };
        let thread = state.thread.as_ref().expect("filtered above");
        let started_at = active_work_started_at(
            thread.latest_turn.as_ref(),
            thread.session.as_ref(),
            self.local_dispatch.as_ref().map(|d| d.started_at.as_str()),
        );
        let optimistic: Vec<Arc<OrchestrationMessage>> = self
            .local_dispatch
            .iter()
            .filter_map(|dispatch| dispatch.message.clone())
            .collect();
        let scope = self.thread_key();
        self.timeline.sync(
            state.clone(),
            &scope,
            &optimistic,
            is_working,
            started_at.as_deref(),
        );
        cx.notify();
    }

    fn thread_key(&self) -> String {
        match &self.target {
            ChatTarget::Thread(thread) => thread.key(),
            ChatTarget::Draft { id, .. } => format!("draft:{}", id.0),
        }
    }

    /// Re-renders once a second while a "Working for" timer is visible (`WorkingTimer`).
    fn tick_working_timer(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |this, cx| {
                    if this.timeline.has_working_row() && this.app_state.read(cx).clock_is_live() {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    /// The composer overlay measured `height` (ceiled): updates the timeline inset.
    fn set_overlay_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        let height = height.ceil();
        if height <= px(0.) {
            return;
        }
        self.overlay_height = height;
        let inset =
            resolve_composer_timeline_inset(self.timeline_inset, height, self.composer_resting);
        if inset != self.timeline_inset {
            self.timeline_inset = inset;
            cx.notify();
        }
    }

    /// The composer entered (`true`) or left its resting layout. Only the flag is stored: the
    /// next overlay measurement applies it.
    pub fn set_composer_resting(&mut self, resting: bool, _cx: &mut Context<Self>) {
        self.composer_resting = resting;
    }

    /// The composer's queued follow-ups for this thread, oldest first.
    pub fn set_queued_messages(&mut self, queued: Vec<QueuedMessage>, cx: &mut Context<Self>) {
        if self.queued_messages != queued {
            self.queued_messages = queued;
            self.refresh_rows(cx);
        }
    }

    /// Shows `error` in the thread error banner (or clears it) without touching a send in
    /// flight: attachment, stash, and interrupt failures.
    pub fn set_thread_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        if self.local_error != error {
            self.local_error = error;
            cx.notify();
        }
    }

    /// PageUp (`up`) / PageDown typed in the composer when its editor cannot scroll: scrolls
    /// the timeline by one page (viewport minus the overlay minus 36px,
    /// `pageScrollController.ts`). Returns whether the timeline could scroll that way.
    pub fn scroll_timeline_page(&mut self, up: bool, cx: &mut Context<Self>) -> bool {
        const ALIGNMENT_OFFSET: f32 = 36.;
        let list = &self.timeline.list;
        let viewport = list.viewport_bounds().size.height;
        let scroll_top = -list.scroll_px_offset_for_scrollbar().y;
        let max = list.max_offset_for_scrollbar().y;
        let can_scroll = if up {
            scroll_top > px(1.)
        } else {
            scroll_top < max - px(1.)
        };
        if !can_scroll {
            return false;
        }
        let page = (viewport - px(ALIGNMENT_OFFSET) - self.overlay_height).max(px(0.));
        if up {
            self.timeline.manual_navigation = true;
            list.scroll_by(-page);
            cx.emit(ChatEvent::CollapseComposer);
        } else {
            list.scroll_by(page);
        }
        cx.notify();
        true
    }
}

impl EventEmitter<ChatEvent> for ChatView {}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let loaded = match &self.target {
            ChatTarget::Thread(_) => self.orchestration_thread().is_some(),
            ChatTarget::Draft { .. } => true,
        };
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            // The route renders nothing until the thread's first snapshot arrives.
            .when(loaded, |this| {
                this.child(self.render_header(window, cx))
                    .children(self.render_error_banner(cx))
                    .child(self.render_body(window, cx))
            })
    }
}
