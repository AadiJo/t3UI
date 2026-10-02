//! Process-wide bounded cache of highlighted code, mirroring the fork's LRU
//! (500 entries / 50 MB, `ChatMarkdown.tsx:124-143`).

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    sync::{Arc, LazyLock},
};

use parking_lot::Mutex;

use crate::{Highlighted, Language, Theme};

const MAX_ENTRIES: usize = 500;
const MAX_COST: usize = 50 * 1024 * 1024;

/// Cache key: a 64-bit content hash plus the length, like the fork's `fnv1a32:length` key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    hash: u64,
    len: usize,
    language: Language,
    theme: Theme,
}

impl Key {
    pub(crate) fn new(code: &str, language: Language, theme: Theme) -> Self {
        let mut hasher = DefaultHasher::new();
        code.hash(&mut hasher);
        Self {
            hash: hasher.finish(),
            len: code.len(),
            language,
            theme,
        }
    }
}

struct Entry {
    value: Arc<Highlighted>,
    cost: usize,
    last_used: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    total_cost: usize,
    clock: u64,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);

pub(crate) fn get(key: &Key) -> Option<Arc<Highlighted>> {
    let mut cache = CACHE.lock();
    cache.clock += 1;
    let clock = cache.clock;
    let entry = cache.entries.get_mut(key)?;
    entry.last_used = clock;
    Some(entry.value.clone())
}

pub(crate) fn insert(key: Key, value: Arc<Highlighted>, cost: usize) {
    if cost > MAX_COST {
        return;
    }
    let mut cache = CACHE.lock();
    cache.clock += 1;
    let last_used = cache.clock;
    if let Some(previous) = cache.entries.insert(
        key,
        Entry {
            value,
            cost,
            last_used,
        },
    ) {
        cache.total_cost -= previous.cost;
    }
    cache.total_cost += cost;
    while cache.entries.len() > MAX_ENTRIES || cache.total_cost > MAX_COST {
        // A linear scan is fine at 500 entries and keeps this dependency-free.
        let Some(oldest) = cache
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| *key)
        else {
            break;
        };
        if let Some(evicted) = cache.entries.remove(&oldest) {
            cache.total_cost -= evicted.cost;
        }
    }
}
