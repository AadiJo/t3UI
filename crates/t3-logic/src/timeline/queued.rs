//! Follow-up messages queued while a turn runs (`QueuedComposerMessage`, display fields only).
//! The composer owns the queue and pushes each thread's list to its chat view, which renders
//! them as dashed rows at the end of the timeline (`MessagesTimeline.tsx:1796-1896`).

/// One queued follow-up as the timeline shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedMessage {
    /// Queue entry id; the row key is `queued-message:{id}`.
    pub id: String,
    /// ISO time the message was queued.
    pub created_at: String,
    /// The prompt text (rendered like a user message body when non-empty after trimming).
    pub prompt: String,
    /// Images plus files.
    pub attachment_count: usize,
    /// Terminal contexts, preview annotations, and review comments.
    pub context_count: usize,
    /// The queue is sending this message to the agent right now.
    pub sending: bool,
    /// Stays queued until the user presses "Send now" (Stop left it behind).
    pub hold_until_user_action: bool,
}

impl QueuedMessage {
    /// The status tooltip under the bubble. `is_next` marks the oldest queued message.
    pub fn status_label(&self, is_next: bool) -> &'static str {
        if self.sending {
            "Sending to the agent"
        } else if self.hold_until_user_action {
            "Waits for Send now"
        } else if is_next {
            "Sends when the turn ends"
        } else {
            "Sends after the messages above it"
        }
    }

    /// "2 attachments, 1 context item", or `None` when there are neither.
    pub fn counts_label(&self) -> Option<String> {
        let plural = |count: usize, noun: &str| {
            format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
        };
        let parts: Vec<String> = [
            (self.attachment_count > 0).then(|| plural(self.attachment_count, "attachment")),
            (self.context_count > 0).then(|| plural(self.context_count, "context item")),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}
