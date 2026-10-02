//! Ranked search shared by the command menu and the model picker
//! (`shared/searchRanking.ts`, `composerSlashCommandSearch.ts`, `providerSkillSearch.ts`,
//! `modelPickerSearch.ts`). Lower scores rank first; `None` means no match.
//!
//! Positions are counted in chars, which equals the web's UTF-16 indexes for the ASCII names
//! these lists hold.

use t3_protocol::server::ProviderSkill;

/// Trims and lowercases a query, optionally dropping leading sigils (`/`, `$`).
pub fn normalize_query(input: &str, strip_leading: Option<char>) -> String {
    let trimmed = input.trim();
    let trimmed = match strip_leading {
        Some(sigil) => trimmed.trim_start_matches(sigil),
        None => trimmed,
    };
    trimmed.to_lowercase()
}

/// Base scores per match tier (`scoreQueryMatch` input). `None` disables a tier.
#[derive(Clone, Copy, Debug)]
pub struct MatchTiers {
    pub exact: i64,
    pub prefix: Option<i64>,
    pub boundary: Option<i64>,
    pub includes: Option<i64>,
    pub fuzzy: Option<i64>,
    pub boundary_markers: &'static [char],
}

impl MatchTiers {
    /// exact `base`, prefix `base+2`, boundary `base+4`, includes `base+6`, no fuzzy.
    pub const fn standard(base: i64) -> Self {
        Self {
            exact: base,
            prefix: Some(base + 2),
            boundary: Some(base + 4),
            includes: Some(base + 6),
            fuzzy: None,
            boundary_markers: &[' ', '-', '_', '/'],
        }
    }

    pub const fn with_fuzzy(mut self, base: i64) -> Self {
        self.fuzzy = Some(base);
        self
    }

    pub const fn with_markers(mut self, markers: &'static [char]) -> Self {
        self.boundary_markers = markers;
        self
    }

    pub const fn without_boundary(mut self) -> Self {
        self.boundary = None;
        self
    }
}

/// Scores `value` against `query`; both must already be normalized (`scoreQueryMatch`).
pub fn score_match(value: &str, query: &str, tiers: MatchTiers) -> Option<i64> {
    if value.is_empty() || query.is_empty() {
        return None;
    }
    if value == query {
        return Some(tiers.exact);
    }
    let length_penalty = (char_len(value) as i64 - char_len(query) as i64).clamp(0, 64);
    if let Some(prefix) = tiers.prefix
        && value.starts_with(query)
    {
        return Some(prefix + length_penalty);
    }
    if let Some(boundary) = tiers.boundary {
        let best = tiers
            .boundary_markers
            .iter()
            .filter_map(|marker| {
                value
                    .find(&format!("{marker}{query}"))
                    .map(|index| char_index(value, index) + 1)
            })
            .min();
        if let Some(index) = best {
            return Some(boundary + index as i64 * 2 + length_penalty);
        }
    }
    if let Some(includes) = tiers.includes
        && let Some(index) = value.find(query)
    {
        return Some(includes + char_index(value, index) as i64 * 2 + length_penalty);
    }
    if let Some(fuzzy) = tiers.fuzzy
        && let Some(score) = score_subsequence(value, query)
    {
        return Some(fuzzy + score);
    }
    None
}

/// Fuzzy subsequence score: earlier, tighter matches score lower (`scoreSubsequenceMatch`).
pub fn score_subsequence(value: &str, query: &str) -> Option<i64> {
    let query: Vec<char> = query.chars().collect();
    if query.is_empty() {
        return Some(0);
    }
    let value: Vec<char> = value.chars().collect();
    let (mut query_index, mut first, mut previous, mut gaps) = (0, None, None, 0i64);
    for (index, ch) in value.iter().enumerate() {
        if *ch != query[query_index] {
            continue;
        }
        let first = *first.get_or_insert(index);
        if let Some(previous) = previous {
            gaps += (index - previous - 1) as i64;
        }
        previous = Some(index);
        query_index += 1;
        if query_index == query.len() {
            let span = (index - first + 1 - query.len()) as i64;
            let length = (value.len() as i64 - query.len() as i64).min(64);
            return Some(first as i64 * 2 + gaps * 3 + span + length);
        }
    }
    None
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn char_index(text: &str, byte_index: usize) -> usize {
    text[..byte_index].chars().count()
}

/// Sorts scored items by score, then tie-breaker, keeping insertion order on full ties.
pub fn rank<T>(mut scored: Vec<(i64, String, T)>) -> Vec<T> {
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

/// Score of a slash command against a normalized query: the name (fuzzy) or the description.
pub fn score_slash_command(name: &str, description: &str, query: &str) -> Option<i64> {
    let name_tiers = MatchTiers::standard(0)
        .with_fuzzy(100)
        .with_markers(&['-', '_', '/']);
    [
        score_match(&name.to_lowercase(), query, name_tiers),
        score_match(&description.to_lowercase(), query, MatchTiers::standard(20)),
    ]
    .into_iter()
    .flatten()
    .min()
}

/// Enabled skills matching `query` (`$` stripped), best first (`searchProviderSkills`). An empty
/// query lists every enabled skill in server order.
pub fn search_skills<'a>(skills: &'a [ProviderSkill], query: &str) -> Vec<&'a ProviderSkill> {
    let enabled = skills.iter().filter(|skill| skill.enabled);
    let query = normalize_query(query, Some('$'));
    if query.is_empty() {
        return enabled.collect();
    }
    let scored = enabled
        .filter_map(|skill| {
            let label = skill_display_name(skill);
            let tiers = [
                score_match(
                    &skill.name.to_lowercase(),
                    &query,
                    MatchTiers::standard(0)
                        .with_fuzzy(100)
                        .with_markers(&['-', '_', '/']),
                ),
                score_match(
                    &label.to_lowercase(),
                    &query,
                    MatchTiers::standard(1).with_fuzzy(110),
                ),
                score_match(
                    &skill
                        .short_description
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase(),
                    &query,
                    MatchTiers::standard(20),
                ),
                score_match(
                    &skill
                        .description
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase(),
                    &query,
                    MatchTiers::standard(30),
                ),
                score_match(
                    &skill.scope.as_deref().unwrap_or_default().to_lowercase(),
                    &query,
                    MatchTiers::standard(40).without_boundary(),
                ),
            ];
            let score = tiers.into_iter().flatten().min()?;
            let tie = format!("{}\u{0}{}", label.to_lowercase(), skill.name);
            Some((score, tie, skill))
        })
        .collect();
    rank(scored)
}

/// A skill's label: its `displayName`, else the name title-cased on `space : _ -`.
pub fn skill_display_name(skill: &ProviderSkill) -> String {
    if let Some(display) = skill.display_name.as_deref().map(str::trim)
        && !display.is_empty()
    {
        return display.to_owned();
    }
    title_case_words(&skill.name)
}

/// Where a skill comes from, shown at the right of its menu row ("App", "System", ...).
pub fn skill_install_source(skill: &ProviderSkill) -> Option<String> {
    let path = skill.path.replace('\\', "/");
    if path.contains("/.codex/plugins/") || path.contains("/.agents/plugins/") {
        return Some("App".into());
    }
    let scope = skill.scope.as_deref()?.trim().to_lowercase();
    Some(match scope.as_str() {
        "" => return None,
        "system" => "System".into(),
        "project" | "workspace" | "local" => "Project".into(),
        "user" | "personal" => "Personal".into(),
        other => title_case_words(other),
    })
}

fn title_case_words(value: &str) -> String {
    value
        .split(|c: char| c.is_whitespace() || matches!(c, ':' | '_' | '-'))
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Searchable fields of one model-picker row.
#[derive(Clone, Debug)]
pub struct ModelSearchFields<'a> {
    pub name: &'a str,
    pub short_name: Option<&'a str>,
    pub sub_provider: Option<&'a str>,
    pub driver: &'a str,
    pub provider_display_name: &'a str,
    pub is_favorite: bool,
}

impl ModelSearchFields<'_> {
    /// The combined text, also the tie-breaker (`buildModelPickerSearchText`).
    pub fn search_text(&self) -> String {
        [
            Some(self.name),
            self.short_name,
            self.sub_provider,
            Some(self.driver),
            Some(self.provider_display_name),
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_lowercase()
    }
}

/// Tokenized model search (`scoreModelPickerSearch`): each token takes its best field score
/// (field base = index × 10), the scores add up, and favorites get −24.
pub fn score_model(fields: &ModelSearchFields<'_>, query: &str) -> Option<i64> {
    let query = normalize_query(query, None);
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return Some(0);
    }
    let normalized = |value: &str| value.trim().to_lowercase();
    let values: Vec<String> = [Some(fields.name), fields.short_name, fields.sub_provider]
        .into_iter()
        .flatten()
        .map(normalized)
        .chain([
            normalized(fields.driver),
            normalized(fields.provider_display_name),
            fields.search_text(),
        ])
        .collect();
    let mut total = 0;
    for token in tokens {
        let best = values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let base = index as i64 * 10;
                let mut tiers = MatchTiers::standard(base);
                if token.chars().count() >= 3 {
                    tiers = tiers.with_fuzzy(base + 100);
                }
                score_match(value, token, tiers)
            })
            .min()?;
        total += best;
    }
    Some(if fields.is_favorite {
        total - 24
    } else {
        total
    })
}
