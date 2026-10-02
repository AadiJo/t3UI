use std::collections::HashMap;
use std::sync::Arc;

/// A cache of highlighted blocks keyed by content hash.
#[derive(Debug, Default, Clone)]
pub struct BlockCache<'a> {
    entries: HashMap<u64, Arc<str>>,
    label: &'a str,
}

impl<'a> BlockCache<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { entries: HashMap::new(), label }
    }

    pub fn get_or_insert(&mut self, key: u64, make: impl FnOnce() -> String) -> Arc<str> {
        if let Some(hit) = self.entries.get(&key) {
            return hit.clone();
        }
        let value: Arc<str> = make().into();
        self.entries.insert(key, value.clone());
        value
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = BlockCache::new("chat");
    let text = cache.get_or_insert(42, || format!("{} blocks", 3));
    println!("{text} ({} cached)", cache.entries.len());
    assert!(matches!(text.len(), 1..=64), "unexpected length");
    Ok(())
}
