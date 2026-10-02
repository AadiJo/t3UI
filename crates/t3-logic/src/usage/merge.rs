//! Merges every environment's usage summary into the single view the page renders
//! (`packages/shared/src/usageMerge.ts`). Environments on one machine read the same transcript
//! directories, so sources are claimed per fingerprint before anything is summed.

use std::collections::{BTreeMap, HashMap, HashSet};

use t3_protocol::usage::{CostSource, UsageBucket, UsageSource, UsageSummary};

use super::ProviderKind;
use crate::time::parse_timestamp;

/// The usage contract this client speaks (`USAGE_CONTRACT_VERSION`).
pub const USAGE_CONTRACT_VERSION: u32 = 6;
/// The oldest summary version that still merges (`USAGE_MERGE_COMPATIBLE_SINCE`).
pub const USAGE_MERGE_COMPATIBLE_SINCE: u32 = 4;

/// Whether a summary of `version` merges into this client's totals.
pub fn is_compatible_version(version: u32) -> bool {
    (USAGE_MERGE_COMPATIBLE_SINCE..=USAGE_CONTRACT_VERSION).contains(&version)
}

/// One environment's answer.
#[derive(Clone, Debug)]
pub struct EnvironmentUsage {
    pub environment_id: String,
    pub label: String,
    pub summary: UsageSummary,
}

/// Per-provider totals.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderTotals {
    pub provider: ProviderKind,
    pub cost_usd: f64,
    pub total_tokens: u64,
    pub records: u64,
    pub sessions: u64,
    pub cost_share: f64,
    pub token_share: f64,
}

/// Per-model totals.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelTotals {
    pub model: String,
    pub provider: ProviderKind,
    pub cost_usd: f64,
    pub total_tokens: u64,
    pub records: u64,
    pub unpriced_records: u64,
    pub cost_share: f64,
}

impl ModelTotals {
    /// Every record lacked rates: the cost is unknown, not zero ("Unpriced").
    pub fn is_cost_unknown(&self) -> bool {
        self.records > 0 && self.unpriced_records >= self.records
    }
}

/// Cost and tokens of one provider in one period.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PeriodValue {
    pub cost_usd: f64,
    pub total_tokens: u64,
}

/// One day or hour. `period` is the day (`YYYY-MM-DD`) or the hour start as the server sent it;
/// `start_millis` is set for hours so the chart can match them by instant.
#[derive(Clone, Debug, PartialEq)]
pub struct PeriodTotals {
    pub period: String,
    pub day: String,
    pub start_millis: Option<i64>,
    pub cost_usd: f64,
    pub total_tokens: u64,
    pub by_provider: BTreeMap<ProviderKind, PeriodValue>,
}

/// How good the cost figure is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CostQuality {
    pub provider_reported_share: f64,
    pub model_priced_share: f64,
    pub unpriced_share: f64,
    pub cache_savings_usd: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractMismatchDirection {
    /// The server is older than the oldest version this client merges.
    ServerBehind,
    /// The server speaks a newer contract than this client.
    ClientBehind,
}

/// An environment excluded from totals because its contract version does not merge.
#[derive(Clone, Debug, PartialEq)]
pub struct ContractMismatch {
    pub environment_id: String,
    pub direction: ContractMismatchDirection,
    pub contract_version: u32,
}

impl ContractMismatch {
    /// The coverage notice line (`formatUsageContractMismatch`).
    pub fn message(&self, environment_label: &str) -> String {
        match self.direction {
            ContractMismatchDirection::ServerBehind => format!(
                "{environment_label} runs an older server version and is excluded from totals."
            ),
            ContractMismatchDirection::ClientBehind => format!(
                "This client is older than the server on {environment_label}; its usage is excluded from totals."
            ),
        }
    }
}

/// The merged view (`MergedUsage`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergedUsage {
    pub cost_usd: f64,
    pub uncached_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
    pub records: u64,
    pub sessions: u64,
    /// Sorted by cost, highest first.
    pub providers: Vec<ProviderTotals>,
    /// Sorted by cost, then tokens.
    pub models: Vec<ModelTotals>,
    /// Ascending by day.
    pub daily: Vec<PeriodTotals>,
    /// Ascending by hour start; empty for daily windows.
    pub hourly: Vec<PeriodTotals>,
    pub cost_quality: CostQuality,
    /// `"<label>: <path>"` of sources dropped as duplicates of another environment's.
    pub duplicate_sources: Vec<String>,
    pub contributing_environments: Vec<String>,
    pub contract_mismatches: Vec<ContractMismatch>,
}

impl MergedUsage {
    /// Providers with real activity in [`ProviderKind::ORDER`] (`providersWithUsage`).
    pub fn active_providers(&self) -> Vec<ProviderKind> {
        ProviderKind::ORDER
            .into_iter()
            .filter(|kind| {
                self.providers.iter().any(|totals| {
                    totals.provider == *kind && (totals.total_tokens > 0 || totals.cost_usd > 0.)
                })
            })
            .collect()
    }

    pub fn provider(&self, kind: ProviderKind) -> Option<&ProviderTotals> {
        self.providers.iter().find(|totals| totals.provider == kind)
    }
}

/// Identity of a physical transcript directory: host, provider, path and filesystem id.
type FingerprintKey = (String, String, String, String);
/// A cell: `(day, hour start, provider, model)`.
type CellKey = (String, Option<String>, String, String);

fn fingerprint_key(source: &UsageSource) -> FingerprintKey {
    let fingerprint = &source.fingerprint;
    (
        fingerprint.host_id.clone(),
        fingerprint.provider.as_str().to_owned(),
        fingerprint.resolved_home_path.clone(),
        fingerprint.volume_id.clone(),
    )
}

fn cell_key(bucket: &UsageBucket) -> CellKey {
    (
        bucket.day.clone(),
        bucket.hour_start.clone(),
        bucket.provider.as_str().to_owned(),
        bucket.model.clone(),
    )
}

fn read_at(environment: &EnvironmentUsage) -> i64 {
    parse_timestamp(&environment.summary.read_at).unwrap_or(0)
}

/// Buckets one source contributed: those naming its directory, or unattributed ones when it is
/// the provider's only source in the summary.
fn buckets_for_source<'a>(
    summary: &'a UsageSummary,
    source: &UsageSource,
) -> impl Iterator<Item = &'a UsageBucket> {
    let provider = source.fingerprint.provider.clone();
    let path = source.fingerprint.resolved_home_path.clone();
    let only_source = summary
        .sources
        .iter()
        .filter(|entry| entry.fingerprint.provider == provider)
        .count()
        == 1;
    summary.buckets.iter().filter(move |bucket| {
        bucket.provider == provider
            && match &bucket.source_path {
                Some(source_path) => *source_path == path,
                None => only_source,
            }
    })
}

/// Who owns each directory (`claimSources`). Indices refer to `environments`.
struct Claims {
    owner: HashMap<FingerprintKey, usize>,
    /// Buckets (by index into the summary) a newer partial scan adds to an older complete one.
    supplemental: HashMap<usize, HashSet<usize>>,
    sessions: HashMap<FingerprintKey, u64>,
    duplicates: Vec<String>,
}

fn claim_sources(environments: &[&EnvironmentUsage]) -> Claims {
    let mut order: Vec<usize> = (0..environments.len()).collect();
    order.sort_by(|&a, &b| {
        read_at(environments[b])
            .cmp(&read_at(environments[a]))
            .then_with(|| {
                environments[a]
                    .environment_id
                    .cmp(&environments[b].environment_id)
            })
    });

    let mut claims = Claims {
        owner: HashMap::new(),
        supplemental: HashMap::new(),
        sessions: HashMap::new(),
        duplicates: Vec::new(),
    };
    let mut owner_source: HashMap<FingerprintKey, (usize, &UsageSource)> = HashMap::new();
    // Complete scans claim first, then partial, then failed; newest first within each.
    for status in ["ok", "partial", "failed"] {
        for &index in &order {
            let environment = environments[index];
            for source in &environment.summary.sources {
                if source.status != status
                    || ProviderKind::from_wire(&source.fingerprint.provider).is_none()
                {
                    continue;
                }
                let key = fingerprint_key(source);
                if claims.owner.contains_key(&key) {
                    claims.duplicates.push(format!(
                        "{}: {}",
                        environment.label, source.fingerprint.resolved_home_path
                    ));
                    continue;
                }
                claims.owner.insert(key.clone(), index);
                owner_source.insert(key.clone(), (index, source));
                claims.sessions.insert(key, source.distinct_sessions);
            }
        }
    }

    // A newer partial scan keeps cells the older complete owner lacks.
    let mut seen: HashMap<FingerprintKey, HashSet<CellKey>> = HashMap::new();
    for &index in &order {
        let environment = environments[index];
        for source in &environment.summary.sources {
            if source.status != "partial" {
                continue;
            }
            let key = fingerprint_key(source);
            let Some(&(owner_index, owner)) = owner_source.get(&key) else {
                continue;
            };
            if owner.status != "ok" || read_at(environment) <= read_at(environments[owner_index]) {
                continue;
            }
            let cells = seen.entry(key.clone()).or_insert_with(|| {
                buckets_for_source(&environments[owner_index].summary, owner)
                    .map(cell_key)
                    .collect()
            });
            let mut added = false;
            for bucket in buckets_for_source(&environment.summary, source) {
                if !cells.insert(cell_key(bucket)) {
                    continue;
                }
                let position = environment
                    .summary
                    .buckets
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, bucket))
                    .unwrap_or_default();
                claims
                    .supplemental
                    .entry(index)
                    .or_default()
                    .insert(position);
                added = true;
            }
            if added {
                let sessions = claims.sessions.entry(key).or_default();
                *sessions = (*sessions).max(source.distinct_sessions);
            }
        }
    }
    claims
}

fn bucket_tokens(bucket: &UsageBucket) -> u64 {
    // Reasoning is a subset of output; never add it again.
    let totals = &bucket.totals;
    totals.uncached_input_tokens
        + totals.cached_input_tokens
        + totals.cache_creation_tokens
        + totals.output_tokens
}

fn share(part: f64, whole: f64) -> f64 {
    if whole == 0. { 0. } else { part / whole }
}

#[derive(Default)]
struct ProviderAccumulator {
    cost_usd: f64,
    total_tokens: u64,
    records: u64,
    sessions: u64,
}

#[derive(Default)]
struct ModelAccumulator {
    cost_usd: f64,
    total_tokens: u64,
    records: u64,
    unpriced_records: u64,
}

fn add_to_period(
    periods: &mut HashMap<String, PeriodTotals>,
    key: String,
    day: &str,
    start_millis: Option<i64>,
    provider: ProviderKind,
    bucket: &UsageBucket,
    tokens: u64,
) {
    let period = periods.entry(key.clone()).or_insert_with(|| PeriodTotals {
        period: key,
        day: day.to_owned(),
        start_millis,
        cost_usd: 0.,
        total_tokens: 0,
        by_provider: BTreeMap::new(),
    });
    period.cost_usd += bucket.cost_usd;
    period.total_tokens += tokens;
    let value = period.by_provider.entry(provider).or_default();
    value.cost_usd += bucket.cost_usd;
    value.total_tokens += tokens;
}

/// Merges every connected environment's summary (`mergeUsage`). Summaries outside the
/// compatible contract range are excluded and reported, never merged.
pub fn merge_usage(environments: &[EnvironmentUsage]) -> MergedUsage {
    let mut merged = MergedUsage::default();
    let mut current = Vec::new();
    for environment in environments {
        let version = environment.summary.contract_version;
        if is_compatible_version(version) {
            current.push(environment);
        } else {
            merged.contract_mismatches.push(ContractMismatch {
                environment_id: environment.environment_id.clone(),
                direction: if version < USAGE_CONTRACT_VERSION {
                    ContractMismatchDirection::ServerBehind
                } else {
                    ContractMismatchDirection::ClientBehind
                },
                contract_version: version,
            });
        }
    }
    if current.is_empty() {
        return merged;
    }

    let claims = claim_sources(&current);
    let mut providers: BTreeMap<ProviderKind, ProviderAccumulator> = BTreeMap::new();
    let mut models: HashMap<(ProviderKind, String), ModelAccumulator> = HashMap::new();
    let mut daily: HashMap<String, PeriodTotals> = HashMap::new();
    let mut hourly: HashMap<String, PeriodTotals> = HashMap::new();
    let (mut provider_reported, mut unpriced) = (0_u64, 0_u64);

    for (index, environment) in current.iter().enumerate() {
        // Sources this environment owns, and their sessions per provider.
        let mut owned_providers = HashSet::new();
        let mut owned_sources = HashSet::new();
        for source in &environment.summary.sources {
            let Some(kind) = ProviderKind::from_wire(&source.fingerprint.provider) else {
                continue;
            };
            if source.status == "missing" {
                continue;
            }
            let key = fingerprint_key(source);
            if claims.owner.get(&key) != Some(&index) {
                continue;
            }
            owned_providers.insert(kind);
            owned_sources.insert((kind, source.fingerprint.resolved_home_path.clone()));
            let sessions = claims
                .sessions
                .get(&key)
                .copied()
                .unwrap_or(source.distinct_sessions);
            merged.sessions += sessions;
            if sessions > 0 {
                providers.entry(kind).or_default().sessions += sessions;
            }
        }
        let supplemental = claims.supplemental.get(&index);
        let mut contributed = false;
        for (position, bucket) in environment.summary.buckets.iter().enumerate() {
            let Some(kind) = ProviderKind::from_wire(&bucket.provider) else {
                continue;
            };
            let owned = supplemental.is_some_and(|set| set.contains(&position))
                || match &bucket.source_path {
                    None => owned_providers.contains(&kind),
                    Some(path) => owned_sources.contains(&(kind, path.clone())),
                };
            if !owned {
                continue;
            }
            contributed = true;
            let tokens = bucket_tokens(bucket);
            merged.cost_usd += bucket.cost_usd;
            merged.cost_quality.cache_savings_usd += bucket.cache_savings_usd;
            merged.uncached_input_tokens += bucket.totals.uncached_input_tokens;
            merged.cached_input_tokens += bucket.totals.cached_input_tokens;
            merged.cache_creation_tokens += bucket.totals.cache_creation_tokens;
            merged.output_tokens += bucket.totals.output_tokens;
            merged.reasoning_tokens += bucket.totals.reasoning_tokens;
            merged.records += bucket.records;
            unpriced += bucket.unpriced_records;
            if bucket.cost_source == CostSource::ProviderReported {
                provider_reported += bucket.records;
            }

            let provider = providers.entry(kind).or_default();
            provider.cost_usd += bucket.cost_usd;
            provider.total_tokens += tokens;
            provider.records += bucket.records;

            let model = models.entry((kind, bucket.model.clone())).or_default();
            model.cost_usd += bucket.cost_usd;
            model.total_tokens += tokens;
            model.records += bucket.records;
            model.unpriced_records += bucket.unpriced_records;

            add_to_period(
                &mut daily,
                bucket.day.clone(),
                &bucket.day,
                None,
                kind,
                bucket,
                tokens,
            );
            if let Some(hour_start) = &bucket.hour_start {
                // Keyed by instant so `...00Z` and `...00.000Z` are one hour.
                let millis = parse_timestamp(hour_start);
                let key = millis.map_or_else(|| hour_start.clone(), |millis| millis.to_string());
                add_to_period(&mut hourly, key, &bucket.day, millis, kind, bucket, tokens);
            }
        }
        if contributed {
            merged
                .contributing_environments
                .push(environment.environment_id.clone());
        }
    }

    merged.total_tokens = merged.uncached_input_tokens
        + merged.cached_input_tokens
        + merged.cache_creation_tokens
        + merged.output_tokens;
    let (cost, tokens) = (merged.cost_usd, merged.total_tokens as f64);

    merged.providers = providers
        .into_iter()
        .map(|(provider, totals)| ProviderTotals {
            provider,
            cost_usd: totals.cost_usd,
            total_tokens: totals.total_tokens,
            records: totals.records,
            sessions: totals.sessions,
            cost_share: share(totals.cost_usd, cost),
            token_share: share(totals.total_tokens as f64, tokens),
        })
        .collect();
    merged
        .providers
        .sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd));

    merged.models = models
        .into_iter()
        .map(|((provider, model), totals)| ModelTotals {
            model,
            provider,
            cost_usd: totals.cost_usd,
            total_tokens: totals.total_tokens,
            records: totals.records,
            unpriced_records: totals.unpriced_records,
            cost_share: share(totals.cost_usd, cost),
        })
        .collect();
    merged.models.sort_by(|a, b| {
        b.cost_usd
            .total_cmp(&a.cost_usd)
            .then(b.total_tokens.cmp(&a.total_tokens))
            .then_with(|| a.provider.cmp(&b.provider))
            .then_with(|| a.model.cmp(&b.model))
    });

    merged.daily = daily.into_values().collect();
    merged.daily.sort_by(|a, b| a.day.cmp(&b.day));
    merged.hourly = hourly.into_values().collect();
    merged.hourly.sort_by(|a, b| {
        a.start_millis
            .cmp(&b.start_millis)
            .then_with(|| a.period.cmp(&b.period))
    });

    let records = merged.records as f64;
    merged.cost_quality.provider_reported_share = share(provider_reported as f64, records);
    merged.cost_quality.unpriced_share = share(unpriced as f64, records);
    merged.cost_quality.model_priced_share = if merged.records == 0 {
        0.
    } else {
        (merged.records - provider_reported - unpriced) as f64 / records
    };
    merged.duplicate_sources = claims.duplicates;
    merged
}

/// Models sorted by tokens (the Tokens metric's breakdown, `sortModelsByTokens`).
pub fn sort_models_by_tokens(models: &[ModelTotals]) -> Vec<ModelTotals> {
    let mut sorted = models.to_vec();
    sorted.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then(b.cost_usd.total_cmp(&a.cost_usd))
    });
    sorted
}
