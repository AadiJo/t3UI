//! String helpers behind the work log (`session-logic.ts`). The web uses regular expressions;
//! these are hand-written equivalents so the crate stays dependency-light. Each function names
//! the expression it replaces.

/// `value.trim()`, or `None` when empty (`asTrimmedString`).
pub(crate) fn trimmed(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// Strips a trailing ` complete` / ` completed` (`/\s+(?:complete|completed)\s*$/i`), then trims.
pub fn normalize_compact_tool_label(value: &str) -> String {
    let end = value.trim_end();
    let lower = end.to_ascii_lowercase();
    let word = ["completed", "complete"]
        .into_iter()
        .find(|word| lower.ends_with(word));
    if let Some(word) = word {
        let before = &end[..end.len() - word.len()];
        let kept = before.trim_end();
        // `\s+` needs at least one whitespace character before the word.
        if kept.len() < before.len() {
            return kept.trim().to_owned();
        }
    }
    value.trim().to_owned()
}

/// Collapses whitespace runs to one space and trims (`normalizeInlinePreview`).
pub(crate) fn normalize_inline_preview(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `normalizePreviewForComparison`: trimmed, whitespace-collapsed, completion suffix removed,
/// lowercased.
pub(crate) fn preview_for_comparison(value: Option<&str>) -> Option<String> {
    let value = trimmed(value?)?;
    Some(normalize_compact_tool_label(&normalize_inline_preview(value)).to_lowercase())
}

/// Truncates to 84 characters with an ellipsis (`truncateInlinePreview`).
pub(crate) fn truncate_inline_preview(value: &str) -> String {
    const MAX: usize = 84;
    if value.chars().count() <= MAX {
        return value.to_owned();
    }
    let head: String = value.chars().take(MAX - 1).collect();
    format!("{}…", head.trim_end())
}

/// Splits a trailing `<exited with exit code N>` off command output (`stripTrailingExitCode`).
/// Returns the trimmed output (`None` when empty) and the exit code when the tag was present.
pub(crate) fn strip_trailing_exit_code(value: &str) -> (Option<String>, Option<u32>) {
    const TAG: &str = "<exited with exit code ";
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if let Some(start) = lower.rfind(TAG) {
        let rest = &value[start + TAG.len()..];
        if let Some(digits) = rest.strip_suffix('>')
            && !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            let output = value[..start].trim();
            let output = (!output.is_empty()).then(|| output.to_owned());
            return (output, digits.parse().ok());
        }
    }
    let output = (!value.is_empty()).then(|| value.to_owned());
    (output, None)
}

/// Error-shaped tool output (`toolDetailTextLooksLikeFailure`): providers often report success
/// while the error text lives in the detail or command.
pub(crate) fn looks_like_failure(text: &str) -> bool {
    let lower = text.to_lowercase();
    const NEEDLES: [&str; 8] = [
        "file not found",
        "no files found",
        "enoent",
        "no such file or directory",
        "no such file",
        "commandnotfoundexception",
        "is not recognized as the name of a cmdlet",
        "a parameter cannot be found that matches parameter name",
    ];
    if NEEDLES.iter().any(|needle| lower.contains(needle))
        || (lower.contains("cannot find path") && lower.contains("because it does not exist"))
        || (lower.contains("is not recognized") && lower.contains("the term '"))
        || lower.contains("command not found")
    {
        return true;
    }
    // `/exit(?:ed)? with exit code\s+[1-9]\d*/i` also covers `<exited with exit code N>`.
    for prefix in ["exited with exit code", "exit with exit code"] {
        for (index, _) in lower.match_indices(prefix) {
            let rest = &lower[index + prefix.len()..];
            let digits = rest.trim_start();
            if digits.len() < rest.len() && starts_with_nonzero_digit(digits) {
                return true;
            }
        }
    }
    // `/exit code\s*[:\s]\s*[1-9]\d*\b/i`
    for (index, _) in lower.match_indices("exit code") {
        let rest = &lower[index + "exit code".len()..];
        let mut tail = rest.trim_start();
        if let Some(after_colon) = tail.strip_prefix(':') {
            tail = after_colon.trim_start();
        }
        if tail.len() < rest.len() && starts_with_nonzero_digit(tail) {
            let after_digits = tail.trim_start_matches(|c: char| c.is_ascii_digit());
            let at_boundary = after_digits
                .chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            if at_boundary {
                return true;
            }
        }
    }
    false
}

fn starts_with_nonzero_digit(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(|byte| (b'1'..=b'9').contains(byte))
}

/// Removes one pair of matching outer quotes (`trimMatchingOuterQuotes`).
fn trim_matching_outer_quotes(value: &str) -> &str {
    let value = value.trim();
    let quoted = (value.starts_with('\'') && value.ends_with('\''))
        || (value.starts_with('"') && value.ends_with('"'));
    if quoted && value.len() >= 2 {
        let inner = value[1..value.len() - 1].trim();
        if !inner.is_empty() {
            return inner;
        }
    }
    value
}

/// Lowercased last path segment of an executable (`executableBasename`).
fn executable_basename(value: &str) -> Option<String> {
    let value = trim_matching_outer_quotes(value);
    let normalized = value.replace('\\', "/");
    let last = normalized.rsplit('/').next().unwrap_or("").trim();
    (!last.is_empty()).then(|| last.to_lowercase())
}

/// `splitExecutableAndRest`: the first (possibly quoted) token and the trimmed remainder.
fn split_executable_and_rest(value: &str) -> Option<(&str, &str)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let close = value[1..].find(quote)? + 1;
        return Some((&value[..=close], value[close + 1..].trim()));
    }
    match value.find(char::is_whitespace) {
        None => Some((value, "")),
        Some(index) => Some((&value[..index], value[index..].trim())),
    }
}

/// Shell wrapper flags: `/(?:^|\s)-command\s+/i`, `/(?:^|\s)\/c\s+/i`, `/(?:^|\s)-(?:l)?c\s+/i`.
fn wrapper_flags(shell: &str) -> Option<&'static [&'static str]> {
    match shell {
        "pwsh" | "pwsh.exe" | "powershell" | "powershell.exe" => Some(&["-command"]),
        "cmd" | "cmd.exe" => Some(&["/c"]),
        "bash" | "sh" | "zsh" => Some(&["-lc", "-c"]),
        _ => None,
    }
}

/// The command after the first wrapper flag (`unwrapCommandRemainder`): the flag must start the
/// string or follow whitespace, and be followed by whitespace.
fn unwrap_command_remainder<'a>(value: &'a str, flags: &[&str]) -> Option<&'a str> {
    let lower = value.to_ascii_lowercase();
    let starts = std::iter::once(0).chain(
        value
            .char_indices()
            .filter(|(_, c)| c.is_whitespace())
            .map(|(index, c)| index + c.len_utf8()),
    );
    for start in starts {
        for flag in flags {
            if !lower[start..].starts_with(flag) {
                continue;
            }
            let after = &value[start + flag.len()..];
            let command = after.trim_start();
            if command.len() == after.len() {
                continue;
            }
            let command = trim_matching_outer_quotes(command.trim());
            return (!command.is_empty()).then_some(command);
        }
    }
    None
}

/// `unwrapKnownShellCommandWrapper`: `bash -lc 'ls'` → `ls`; anything else unchanged.
pub(crate) fn unwrap_shell_wrapper(value: &str) -> String {
    let Some((executable, rest)) = split_executable_and_rest(value) else {
        return value.to_owned();
    };
    if rest.is_empty() {
        return value.to_owned();
    }
    executable_basename(executable)
        .and_then(|shell| wrapper_flags(&shell))
        .and_then(|flags| unwrap_command_remainder(rest, flags))
        .map_or_else(|| value.to_owned(), str::to_owned)
}

/// Quotes one argv part that contains whitespace or quotes (`formatCommandArrayPart`).
pub(crate) fn format_command_array_part(value: &str) -> String {
    if value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '`'))
    {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

/// `encodeURIComponent`, used only to build stable legacy presentation ids.
pub(crate) fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Failure modes: a completion suffix glued to a word ("Autocomplete") being stripped;
    //! exit-code tags that are not at the end; zero exit codes counted as failures; shell
    //! wrappers without a flag (or with the flag inside the command) being unwrapped; quoted
    //! executables; argv parts with spaces left unquoted.
    use super::*;

    #[test]
    fn compact_label_drops_completion_suffix_only_after_whitespace() {
        assert_eq!(
            normalize_compact_tool_label("Ran command complete"),
            "Ran command"
        );
        assert_eq!(
            normalize_compact_tool_label("Read file Completed  "),
            "Read file"
        );
        assert_eq!(normalize_compact_tool_label("Autocomplete"), "Autocomplete");
        assert_eq!(normalize_compact_tool_label("  Web search "), "Web search");
    }

    #[test]
    fn exit_code_tag_is_stripped_only_at_the_end() {
        assert_eq!(
            strip_trailing_exit_code("ok\n<exited with exit code 0>"),
            (Some("ok".into()), Some(0))
        );
        assert_eq!(
            strip_trailing_exit_code("<exited with exit code 2>"),
            (None, Some(2))
        );
        assert_eq!(
            strip_trailing_exit_code("<exited with exit code 2> then more"),
            (Some("<exited with exit code 2> then more".into()), None)
        );
        assert_eq!(strip_trailing_exit_code("   "), (None, None));
    }

    #[test]
    fn failure_heuristics() {
        assert!(looks_like_failure("bash: foo: command not found"));
        assert!(looks_like_failure("Error: ENOENT: no such file"));
        assert!(looks_like_failure("done <exited with exit code 1>"));
        assert!(looks_like_failure("Process exited with exit code 127"));
        assert!(looks_like_failure("exit code: 2"));
        assert!(!looks_like_failure("done <exited with exit code 0>"));
        assert!(!looks_like_failure("exit code 0"));
        assert!(!looks_like_failure("exit code 12abc"));
        assert!(!looks_like_failure("all 12 tests passed"));
    }

    #[test]
    fn shell_wrappers_unwrap_to_the_inner_command() {
        assert_eq!(unwrap_shell_wrapper("/bin/bash -lc 'ls -la'"), "ls -la");
        assert_eq!(unwrap_shell_wrapper("bash -c \"rg -n x\""), "rg -n x");
        assert_eq!(
            unwrap_shell_wrapper("pwsh.exe -NoProfile -Command 'Get-ChildItem'"),
            "Get-ChildItem"
        );
        assert_eq!(unwrap_shell_wrapper("cmd.exe /c dir"), "dir");
        assert_eq!(unwrap_shell_wrapper("\"/usr/local/bin/bash\" -lc ls"), "ls");
        // Only the listed executables count (`bash.exe` is not one).
        assert_eq!(
            unwrap_shell_wrapper("\"C:\\Git\\bash.exe\" -lc ls"),
            "\"C:\\Git\\bash.exe\" -lc ls"
        );
        // No wrapper flag, or not a shell: unchanged.
        assert_eq!(unwrap_shell_wrapper("bash script.sh"), "bash script.sh");
        assert_eq!(unwrap_shell_wrapper("node --test"), "node --test");
        assert_eq!(unwrap_shell_wrapper("grep -c foo"), "grep -c foo");
    }

    #[test]
    fn argv_parts_with_spaces_are_quoted() {
        assert_eq!(format_command_array_part("ls"), "ls");
        assert_eq!(format_command_array_part("a b"), "\"a b\"");
        assert_eq!(
            format_command_array_part("say \"hi\""),
            "\"say \\\"hi\\\"\""
        );
    }

    #[test]
    fn uri_component_encoding_matches_javascript() {
        assert_eq!(encode_uri_component("a b/c\u{1f}"), "a%20b%2Fc%1F");
        assert_eq!(encode_uri_component("Ran-command_(x)"), "Ran-command_(x)");
    }
}
