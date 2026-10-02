//! [`ThreadDetail`]: the open thread's full state (checkpoints, activities, plans) for the right
//! panel surfaces. Live threads follow a `subscribeThread` handle; scenes pass a fixture.

use std::sync::Arc;

use gpui_kit::{Context, Task};
use t3_client::{ThreadHandle, ThreadState};
use t3_protocol::orchestration::OrchestrationThread;

use super::context::PanelContext;

/// The thread surfaces read. Observe it to re-render when the thread changes.
pub struct ThreadDetail {
    state: Option<Arc<ThreadState>>,
    _handle: Option<ThreadHandle>,
    _follow: Option<Task<()>>,
}

impl ThreadDetail {
    /// Subscribes to the thread while this entity lives.
    pub fn live(context: &PanelContext, cx: &mut Context<Self>) -> Self {
        let handle = context.environment(cx).and_then(|environment| {
            environment
                .read(cx)
                .open_thread(context.thread.thread_id.clone())
        });
        let follow = handle.as_ref().map(|handle| {
            let mut receiver = handle.state();
            cx.spawn(async move |this, cx| {
                loop {
                    let state = receiver.borrow_and_update().clone();
                    let applied = this.update(cx, |this, cx| {
                        this.state = Some(state);
                        cx.notify();
                    });
                    if applied.is_err() || receiver.changed().await.is_err() {
                        break;
                    }
                }
            })
        });
        Self {
            state: None,
            _handle: handle,
            _follow: follow,
        }
    }

    /// A fixed state (snapshot fixtures).
    pub fn fixed(state: Arc<ThreadState>) -> Self {
        Self {
            state: Some(state),
            _handle: None,
            _follow: None,
        }
    }

    pub fn state(&self) -> Option<&Arc<ThreadState>> {
        self.state.as_ref()
    }

    /// The thread, once its first snapshot arrived.
    pub fn thread(&self) -> Option<&OrchestrationThread> {
        self.state.as_ref()?.thread.as_ref()
    }
}
