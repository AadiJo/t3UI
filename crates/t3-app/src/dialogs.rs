//! Native message boxes (spec 5.4).

use gpui_kit::{App, PromptButton, PromptLevel, Window};

/// The desktop "confirm" (`LocalApi.dialogs.confirm`): the whole text as the message, buttons
/// No / Yes with No as the default. Resolves to true only for Yes; empty text is false without a
/// dialog.
pub fn confirm(
    message: &str,
    window: &mut Window,
    cx: &mut App,
) -> impl Future<Output = bool> + use<> {
    let answer = (!message.trim().is_empty()).then(|| {
        window.prompt(
            PromptLevel::Info,
            message,
            None,
            &[PromptButton::new("No"), PromptButton::new("Yes")],
            cx,
        )
    });
    async move {
        match answer {
            Some(answer) => answer.await == Ok(1),
            None => false,
        }
    }
}
