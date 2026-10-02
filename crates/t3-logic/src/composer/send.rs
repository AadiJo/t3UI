//! What a send needs to decide before dispatching `thread.turn.start`
//! (`ChatView.logic.ts` `deriveComposerSendState`, `ChatView.tsx` send pipeline,
//! `contracts/orchestration.ts` attachment limits).

use super::prompt::TERMINAL_CONTEXT_PLACEHOLDER;

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

/// The prompt with terminal placeholders removed, trimmed.
pub fn visible_text(prompt: &str) -> String {
    prompt
        .replace(TERMINAL_CONTEXT_PLACEHOLDER, "")
        .trim()
        .to_owned()
}

/// Whether there is anything to send: text, images, or a terminal context with text.
pub fn has_sendable_content(prompt: &str, images: usize, live_terminal_contexts: usize) -> bool {
    !visible_text(prompt).is_empty() || images > 0 || live_terminal_contexts > 0
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
