//! What a send needs to decide before dispatching `thread.turn.start`
//! (`ChatView.logic.ts` `deriveComposerSendState`, `ChatView.tsx` send pipeline,
//! `contracts/orchestration.ts` attachment limits).

use super::prompt::{InlineTokenKind, collect_inline_tokens};

/// Most images one message may carry (the fork's limit; upstream allows more).
pub const MAX_IMAGES_PER_MESSAGE: usize = 8;
/// Largest image accepted, in bytes.
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
/// Text sent for a message that only has images.
pub const IMAGE_ONLY_TEXT: &str = "[User attached one or more images without additional text. \
Respond using the conversation context and the attached image(s).]";
/// Title seed when nothing else names the thread.
pub const DEFAULT_TITLE: &str = "New thread";
/// Error shown when the dispatch fails.
pub const SEND_FAILED: &str = "Failed to send message.";

/// Most characters a turn's input may have (`PROVIDER_SEND_TURN_MAX_INPUT_CHARS`).
pub const MAX_INPUT_CHARS: usize = 120_000;

/// The prompt with context references removed, trimmed (`stripInlineContextReferences`).
pub fn visible_text(prompt: &str) -> String {
    let mut out = String::with_capacity(prompt.len());
    let mut cursor = 0;
    for token in collect_inline_tokens(prompt) {
        if matches!(token.kind, InlineTokenKind::Context { .. }) {
            out.push_str(&prompt[cursor..token.range.start]);
            cursor = token.range.end;
        }
    }
    out.push_str(&prompt[cursor..]);
    out.trim().to_owned()
}

/// Whether there is anything to send: text, images, a terminal context that still has its
/// text, or an element context (`deriveComposerSendState`).
pub fn has_sendable_content(
    prompt: &str,
    images: usize,
    live_terminal_contexts: usize,
    element_contexts: usize,
) -> bool {
    !visible_text(prompt).is_empty()
        || images > 0
        || live_terminal_contexts > 0
        || element_contexts > 0
}

/// The error shown instead of sending an over-long prompt
/// (`getComposerPromptLengthValidationMessage`). Counts UTF-16 units like the web.
pub fn prompt_length_error(prompt: &str) -> Option<String> {
    let length = prompt.trim().encode_utf16().count();
    let excess = length
        .checked_sub(MAX_INPUT_CHARS)
        .filter(|excess| *excess > 0)?;
    let noun = if excess == 1 {
        "character"
    } else {
        "characters"
    };
    Some(format!(
        "Prompt is {} {noun} over the {}-character limit. Shorten or split it before sending.",
        group_thousands(excess),
        group_thousands(MAX_INPUT_CHARS)
    ))
}

/// `1234567` → `1,234,567` (`toLocaleString("en-US")`).
fn group_thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The message text sent for `prompt` with `images` attached.
pub fn outgoing_text(prompt: &str, images: usize) -> String {
    let text = prompt.trim();
    if text.is_empty() && images > 0 {
        IMAGE_ONLY_TEXT.to_owned()
    } else {
        text.to_owned()
    }
}

/// The thread title seed: the text, else "Image: {name}", else "New thread", cut to 50 chars
/// plus "..." (`truncate` in `shared/String.ts`).
pub fn title_seed(prompt: &str, first_image_name: Option<&str>) -> String {
    let text = visible_text(prompt);
    let seed = if !text.is_empty() {
        text
    } else if let Some(name) = first_image_name {
        format!("Image: {name}")
    } else {
        DEFAULT_TITLE.to_owned()
    };
    truncate(&seed, 50)
}

fn truncate(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let cut: String = trimmed.chars().take(max_chars).collect();
    format!("{cut}...")
}

/// Why an image cannot be attached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachError {
    NotAnImage { name: String },
    TooLarge { name: String },
    TooMany,
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnImage { name } => write!(
                f,
                "Unsupported file type for '{name}'. Please attach image files only."
            ),
            Self::TooLarge { name } => write!(f, "'{name}' exceeds the 10MB attachment limit."),
            Self::TooMany => write!(
                f,
                "You can attach up to {MAX_IMAGES_PER_MESSAGE} images per message."
            ),
        }
    }
}

/// Checks one file against the limits, given how many images are already attached.
pub fn check_attachment(
    name: &str,
    mime_type: &str,
    size_bytes: u64,
    already_attached: usize,
) -> Result<(), AttachError> {
    if !mime_type.starts_with("image/") {
        return Err(AttachError::NotAnImage {
            name: name.to_owned(),
        });
    }
    if size_bytes > MAX_IMAGE_BYTES {
        return Err(AttachError::TooLarge {
            name: name.to_owned(),
        });
    }
    if already_attached >= MAX_IMAGES_PER_MESSAGE {
        return Err(AttachError::TooMany);
    }
    Ok(())
}

/// MIME type from a file name's extension, for dropped files.
pub fn image_mime_for_path(path: &str) -> Option<&'static str> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        _ => return None,
    })
}

/// The send button's accessible label (also its tooltip).
pub fn send_button_label(
    environment_unavailable: bool,
    connecting: bool,
    preparing_worktree: bool,
    sending: bool,
) -> &'static str {
    if environment_unavailable {
        "Environment disconnected"
    } else if connecting {
        "Connecting"
    } else if preparing_worktree {
        "Preparing worktree"
    } else if sending {
        "Sending"
    } else {
        "Send message"
    }
}
