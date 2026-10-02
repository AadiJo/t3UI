//! The search box: GitHub-style qualifiers split from free text, local matching for rows already
//! on screen, and relevance ranking (`pullRequestList.logic.ts:162-321,962-1008`).

use std::sync::Arc;

use t3_protocol::pull_requests::{ChecksState, PullRequestListFilters};

use super::EnvironmentEntry;

/// The contract's ceilings on a qualifier: this many names, each this long.
const MAX_QUALIFIER_VALUES: usize = 10;
const MAX_QUALIFIER_LENGTH: usize = 200;

/// A typed query split into the filters the hosts can act on and the text that is left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedQuery {
    pub text: String,
    pub filters: PullRequestListFilters,
}

/// Splits what was typed into qualifiers and text (`parsePullRequestQuery`). Written GitHub's
/// way: `label:foo`, `-label:"needs design"`, `author:octocat`, `draft:true`, `review:approved`,
/// `status:success`. An unknown key is a namespaced label (`size:XXL`), a quoted token is text,
/// and a known key whose value it does not take stays text.
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut text = Vec::new();
    let mut labels: Vec<Vec<String>> = Vec::new();
    let mut excluded: Vec<String> = Vec::new();
    let mut filters = PullRequestListFilters::default();
    for token in tokens(raw) {
        if !apply_qualifier(&token, &mut labels, &mut excluded, &mut filters) {
            text.push(token);
        }
    }
    labels.truncate(MAX_QUALIFIER_VALUES);
    excluded.truncate(MAX_QUALIFIER_VALUES);
    filters.labels = (!labels.is_empty()).then_some(labels);
    filters.excluded_labels = (!excluded.is_empty()).then_some(excluded);
    ParsedQuery {
        text: text.join(" "),
        filters,
    }
}

/// Reads one token as a qualifier. False leaves it as text.
fn apply_qualifier(
    token: &str,
    labels: &mut Vec<Vec<String>>,
    excluded: &mut Vec<String>,
    filters: &mut PullRequestListFilters,
) -> bool {
    let Some((negated, key, raw_value)) = split_qualifier(token) else {
        return false;
    };
    let value = qualifier_value(raw_value);
    if value.is_empty() {
        return false;
    }
    let lower = value.to_lowercase();
    match key.to_lowercase().as_str() {
        "label" => {
            let names = bounded_names(split_qualifier_list(raw_value));
            push_labels(names, negated, labels, excluded)
        }
        "author" => {
            if negated {
                return false;
            }
            filters.author = Some(
                truncate_chars(&value, MAX_QUALIFIER_LENGTH)
                    .trim()
                    .to_owned(),
            );
            true
        }
        "draft" => {
            if negated || (lower != "true" && lower != "false") {
                return false;
            }
            filters.draft = Some(if lower == "true" { "only" } else { "hide" }.to_owned());
            true
        }
        "review" => {
            let decision = match lower.as_str() {
                "approved" => "approved",
                "changes_requested" | "changes-requested" => "changes-requested",
                "required" | "review-required" => "review-required",
                "none" => "none",
                _ => return false,
            };
            if negated {
                return false;
            }
            filters.review = Some(decision.to_owned());
            true
        }
        "status" | "checks" => {
            let state = match lower.as_str() {
                "success" | "passing" => ChecksState::Passing,
                "failure" | "failing" => ChecksState::Failing,
                _ => return false,
            };
            if negated {
                return false;
            }
            filters.checks = Some(state);
            true
        }
        // A pasted link is not a namespace: `https://…` would become a label named `https`.
        _ if value.starts_with('/') => false,
        _ => {
            // The key names the namespace, so the bare parts of `size:S,XS` are both sizes; a
            // part that already has a colon names its whole label.
            let names = bounded_names(
                split_qualifier_list(raw_value)
                    .into_iter()
                    .map(|name| {
                        if name.contains(':') {
                            name
                        } else {
                            format!("{key}:{name}")
                        }
                    })
                    .collect(),
            );
            push_labels(names, negated, labels, excluded)
        }
    }
}

fn push_labels(
    names: Vec<String>,
    negated: bool,
    labels: &mut Vec<Vec<String>>,
    excluded: &mut Vec<String>,
) -> bool {
    if names.is_empty() {
        return false;
    }
    if negated {
        excluded.extend(names);
    } else {
        labels.push(names);
    }
    true
}

/// `(-?)([A-Za-z][A-Za-z0-9_-]*):(.*)`: negation, key, raw value.
fn split_qualifier(token: &str) -> Option<(bool, &str, &str)> {
    let (negated, rest) = match token.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, token),
    };
    let colon = rest.find(':')?;
    let key = &rest[..colon];
    let mut chars = key.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((negated, key, &rest[colon + 1..]))
}

/// Runs of non-space characters in which a closed quoted stretch counts as part of the token,
/// so `label:"needs design"` stays whole. An unbalanced quote separates instead of swallowing
/// the rest of the line (`/(?:[^\s"]|"[^"]*")+/g`).
fn tokens(raw: &str) -> Vec<String> {
    let chars: Vec<char> = raw.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let mut end = index;
        loop {
            match chars.get(end) {
                Some(c) if *c != '"' && !c.is_whitespace() => end += 1,
                Some('"') => match chars[end + 1..].iter().position(|c| *c == '"') {
                    Some(close) => end += close + 2,
                    None => break,
                },
                _ => break,
            }
        }
        if end > index {
            tokens.push(chars[index..end].iter().collect());
            index = end;
        } else {
            index += 1;
        }
    }
    tokens
}

fn qualifier_value(raw: &str) -> String {
    raw.replace('"', "").trim().to_owned()
}

/// A qualifier's value as the list it may be: split on commas unless quoted whole.
fn split_qualifier_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    let quoted_whole = trimmed.len() >= 2
        && trimmed.starts_with('"')
        && trimmed.ends_with('"')
        && !trimmed[1..trimmed.len() - 1].contains('"');
    if quoted_whole {
        let whole = qualifier_value(raw);
        return if whole.is_empty() {
            Vec::new()
        } else {
            vec![whole]
        };
    }
    raw.split(',')
        .map(qualifier_value)
        .filter(|part| !part.is_empty())
        .collect()
}

fn bounded_names(names: Vec<String>) -> Vec<String> {
    names
        .into_iter()
        .take(MAX_QUALIFIER_VALUES)
        .map(|name| {
            truncate_chars(&name, MAX_QUALIFIER_LENGTH)
                .trim()
                .to_owned()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

fn truncate_chars(value: &str, max: usize) -> &str {
    match value.char_indices().nth(max) {
        Some((index, _)) => &value[..index],
        None => value,
    }
}

/// Free-text match over the fields a row shows, plus `#123` / `123` (`matchesPullRequestQuery`).
pub fn matches_query(entry: &EnvironmentEntry, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    format!(
        "#{} {} {} {} {}",
        entry.number,
        entry.title,
        entry.repository,
        entry.head_branch,
        entry
            .author
            .as_ref()
            .map(|author| author.login.as_str())
            .unwrap_or_default()
    )
    .to_lowercase()
    .contains(&needle)
}

/// What [`score_match`] gives a row none of whose own fields carry the text: the host matched
/// something the row does not show, so it wears "matched in the description".
pub const MATCHED_ELSEWHERE_SCORE: u32 = 10;

/// How well a row answers the text (`scorePullRequestMatch`): 100 the number asked for, 90 the
/// exact title, 80 a title containing it, 70 every word in the title, 60 the branch, 50 the
/// author, 40 the repository, 30 any word in the title, 10 nothing the row shows.
pub fn score_match(entry: &EnvironmentEntry, query: &str) -> u32 {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return 0;
    }
    let number = needle.strip_prefix('#').unwrap_or(&needle);
    if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
        return if entry.number.to_string() == number {
            100
        } else {
            0
        };
    }
    let title = entry.title.to_lowercase();
    let terms: Vec<&str> = needle.split_whitespace().collect();
    if title == needle {
        90
    } else if title.contains(&needle) {
        80
    } else if terms.len() > 1 && terms.iter().all(|term| title.contains(term)) {
        70
    } else if entry.head_branch.to_lowercase().contains(&needle) {
        60
    } else if entry
        .author
        .as_ref()
        .is_some_and(|author| author.login.to_lowercase().contains(&needle))
    {
        50
    } else if entry.repository.to_lowercase().contains(&needle) {
        40
    } else if terms.iter().any(|term| title.contains(term)) {
        30
    } else {
        MATCHED_ELSEWHERE_SCORE
    }
}

/// Search results most convincing first, by recency among equals; unchanged without a search.
pub fn rank_matches(entries: &[Arc<EnvironmentEntry>], query: &str) -> Vec<Arc<EnvironmentEntry>> {
    let mut ranked = entries.to_vec();
    if query.trim().is_empty() {
        return ranked;
    }
    ranked.sort_by(|left, right| {
        score_match(right, query)
            .cmp(&score_match(left, query))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    ranked
}
