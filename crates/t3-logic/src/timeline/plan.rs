//! Proposed plan card text (`proposedPlan.ts`): the title, the body without the leading heading
//! and "Summary" heading, and the collapsed preview.

/// The card collapses plans longer than this many characters or [`COLLAPSE_LINES`] lines.
pub const COLLAPSE_CHARS: usize = 900;
pub const COLLAPSE_LINES: usize = 20;

/// The text of a `# Heading` line (up to three leading spaces, `#`..`######`, then whitespace).
fn heading_text(line: &str) -> Option<&str> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after = &rest[hashes..];
    let text = after.trim_start();
    // At least one whitespace character after the hashes, and some text.
    (text.len() < after.len() && !text.is_empty()).then_some(text)
}

/// The first markdown heading anywhere in the plan (`proposedPlanTitle`).
pub fn plan_title(markdown: &str) -> Option<&str> {
    markdown
        .lines()
        .find_map(heading_text)
        .map(str::trim)
        .filter(|title| !title.is_empty())
}

/// Whether the card starts collapsed (more than 900 characters or 20 lines).
pub fn plan_can_collapse(markdown: &str) -> bool {
    markdown.chars().count() > COLLAPSE_CHARS || markdown.split('\n').count() > COLLAPSE_LINES
}

fn drop_blank_lines(lines: &mut Vec<&str>) {
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
}

/// The plan body the card renders: without a leading heading line and a following "Summary"
/// heading (`stripDisplayedPlanMarkdown`).
pub fn displayed_plan_markdown(markdown: &str) -> String {
    let mut lines: Vec<&str> = markdown.trim_end().lines().collect();
    if lines
        .first()
        .is_some_and(|line| heading_text(line).is_some())
    {
        lines.remove(0);
    }
    drop_blank_lines(&mut lines);
    if lines
        .first()
        .and_then(|line| heading_text(line))
        .is_some_and(|text| text.trim().eq_ignore_ascii_case("summary"))
    {
        lines.remove(0);
        drop_blank_lines(&mut lines);
    }
    lines.join("\n")
}

/// The collapsed card's markdown: the first `max_lines` non-blank lines of the body, then a
/// `...` paragraph when more follows (`buildCollapsedProposedPlanPreviewMarkdown`).
pub fn collapsed_plan_preview(markdown: &str, max_lines: usize) -> String {
    let body = displayed_plan_markdown(markdown);
    let mut preview: Vec<&str> = Vec::new();
    let mut visible = 0;
    let mut more = false;
    for line in body.trim_end().lines().map(str::trim_end) {
        let is_visible = !line.trim().is_empty();
        if is_visible && visible >= max_lines {
            more = true;
            break;
        }
        preview.push(line);
        if is_visible {
            visible += 1;
        }
    }
    while preview.last().is_some_and(|line| line.trim().is_empty()) {
        preview.pop();
    }
    if preview.is_empty() {
        return plan_title(markdown)
            .unwrap_or("Plan preview unavailable.")
            .to_owned();
    }
    if more {
        preview.extend(["", "..."]);
    }
    preview.join("\n")
}

#[cfg(test)]
mod tests {
    //! Failure modes: a `#hashtag` (no space) or indented code taken as the title; the
    //! "Summary" heading kept; blank lines counted as preview lines; no ellipsis when trimmed.
    use super::*;

    const PLAN: &str = "# Move aurora-web to a pnpm monorepo\n\n## Summary\n\nSplit the app.\n\n## Steps\n\n1. One\n2. Two\n";

    #[test]
    fn title_is_the_first_heading() {
        assert_eq!(plan_title(PLAN), Some("Move aurora-web to a pnpm monorepo"));
        assert_eq!(plan_title("intro\n  ## Later"), Some("Later"));
        assert_eq!(plan_title("#hashtag only"), None);
        assert_eq!(plan_title("    # indented code"), None);
    }

    #[test]
    fn displayed_body_drops_title_and_summary_heading() {
        assert_eq!(
            displayed_plan_markdown(PLAN),
            "Split the app.\n\n## Steps\n\n1. One\n2. Two"
        );
        assert_eq!(
            displayed_plan_markdown("No heading\ntext"),
            "No heading\ntext"
        );
    }

    #[test]
    fn collapsed_preview_counts_visible_lines() {
        assert_eq!(
            collapsed_plan_preview(PLAN, 3),
            "Split the app.\n\n## Steps\n\n1. One\n\n..."
        );
        assert_eq!(collapsed_plan_preview("# Only a title", 10), "Only a title");
        assert!(!plan_can_collapse(PLAN));
        assert!(plan_can_collapse(&"line\n".repeat(21)));
    }
}
