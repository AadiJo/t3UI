//! Mounts the composer and the branch toolbar into the chat view's slots
//! (`chat::register_slot`). Call [`register_chat_slots`] once at startup.

use gpui_kit::{App, AppContext as _, Entity, IntoElement, Render, Window, div};
use t3_client::commands;
use t3_protocol::orchestration::{MessageRole, OrchestrationMessage};

use super::{BranchToolbar, Composer, ComposerEvent, ComposerTarget, DraftStore};
use crate::{
    chat::{self, ChatTarget, ChatView, Slot},
    state::{AppState, Environment},
};

/// Registers the composer and branch toolbar slot builders.
pub fn register_chat_slots(cx: &mut App) {
    chat::register_slot(
        Slot::Composer,
        |slot, window, cx| {
            let Some((environment, target)) = resolve(&slot.target, cx) else {
                return cx.new(|_| Nothing).into();
            };
            let composer = cx.new(|cx| Composer::new(environment, target, window, cx));
            if let Some(chat) = slot.chat.upgrade() {
                follow_chat(&composer, &chat, cx);
            }
            composer.into()
        },
        cx,
    );
    chat::register_slot(
        Slot::BranchToolbar,
        |slot, _, cx| match resolve(&slot.target, cx) {
            Some((environment, target)) => {
                cx.new(|cx| BranchToolbar::new(environment, target, cx)).into()
            }
            None => cx.new(|_| Nothing).into(),
        },
        cx,
    );
}

/// The composer's environment and target for a chat view.
fn resolve(target: &ChatTarget, cx: &mut App) -> Option<(Entity<Environment>, ComposerTarget)> {
    let app_state = AppState::global(cx);
    match target {
        ChatTarget::Thread(thread) => {
            let environment = app_state.read(cx).environment(&thread.environment_id, cx)?;
            Some((environment, ComposerTarget::Thread(thread.clone())))
        }
        ChatTarget::Draft { id, project } => {
            let environment_id = project
                .as_ref()
                .map(|project| project.environment_id.clone())
                .or_else(|| {
                    DraftStore::global(cx)
                        .read(cx)
                        .draft_thread(id)
                        .map(|draft| draft.environment_id.clone())
                })?;
            let environment = app_state.read(cx).environment(&environment_id, cx)?;
            Some((environment, ComposerTarget::Draft(id.clone())))
        }
    }
}

/// Feeds the chat view's thread into the composer and reports sends back to it.
fn follow_chat(composer: &Entity<Composer>, chat: &Entity<ChatView>, cx: &mut App) {
    if let Some(state) = chat.read(cx).thread().cloned() {
        composer.update(cx, |composer, cx| composer.set_thread_state(state, cx));
    }
    let weak_composer = composer.downgrade();
    cx.observe(chat, move |chat, cx| {
        let Some(state) = chat.read(cx).thread().cloned() else {
            return;
        };
        weak_composer
            .update(cx, |composer, cx| composer.set_thread_state(state, cx))
            .ok();
    })
    .detach();
    let weak_chat = chat.downgrade();
    cx.subscribe(composer, move |_, event: &ComposerEvent, cx| {
        let Some(chat) = weak_chat.upgrade() else {
            return;
        };
        chat.update(cx, |chat, cx| match event {
            ComposerEvent::Sending {
                message_id, text, ..
            } => {
                let now = commands::now();
                let message = OrchestrationMessage {
                    id: message_id.clone(),
                    role: MessageRole::User,
                    text: text.clone(),
                    attachments: None,
                    context: None,
                    turn_id: None,
                    streaming: false,
                    created_at: now.clone(),
                    updated_at: now,
                };
                chat.begin_local_dispatch(Some(message), cx);
            }
            ComposerEvent::SendFailed(message) | ComposerEvent::Error(message) => {
                chat.end_local_dispatch(Some(message.to_string()), cx);
            }
            _ => {}
        });
    })
    .detach();
}

/// An empty slot (no environment or target to compose for).
struct Nothing;

impl Render for Nothing {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        div()
    }
}
