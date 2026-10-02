//! The plan surface (spec 4.1, `PlanSidebar.tsx`). STUB: replaced by the plan implementation.

use gpui_kit::{Context, Entity, IntoElement, Render, Window, div};

use super::{context::PanelContext, thread_detail::ThreadDetail};

/// The `plan` tab: the thread's active plan and its steps.
pub struct PlanSurface {
    _context: PanelContext,
    _detail: Entity<ThreadDetail>,
}

impl PlanSurface {
    pub fn new(
        context: PanelContext,
        detail: Entity<ThreadDetail>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            _context: context,
            _detail: detail,
        }
    }
}

impl Render for PlanSurface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
