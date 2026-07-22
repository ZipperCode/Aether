use regex::{Regex, RegexBuilder};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

const MODEL_MAPPING_CACHE_CAPACITY: usize = 1024;
const MODEL_MAPPING_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug)]
struct CompiledModelMapping {
    pattern: String,
    literal: bool,
    regex: Option<Regex>,
}

impl CompiledModelMapping {
    fn compile(pattern: String) -> Self {
        let literal = !pattern.chars().any(is_regex_meta_character);
        let regex = if literal {
            None
        } else {
            RegexBuilder::new(&format!("^(?:{pattern})$"))
                .case_insensitive(true)
                .build()
                .ok()
        };
        MODEL_MAPPING_RULE_COMPILES.fetch_add(1, Ordering::Relaxed);
        Self {
            pattern,
            literal,
            regex,
        }
    }

    fn is_valid(&self) -> bool {
        self.literal || self.regex.is_some()
    }

    fn is_match(&self, model_name: &str) -> bool {
        self.pattern.eq_ignore_ascii_case(model_name)
            || self
                .regex
                .as_ref()
                .is_some_and(|regex| regex.is_match(model_name))
    }
}

fn is_regex_meta_character(character: char) -> bool {
    matches!(
        character,
        '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' | '^' | '$'
    )
}

#[derive(Debug)]
pub struct CompiledModelMappings {
    rules: Vec<CompiledModelMapping>,
}

impl CompiledModelMappings {
    pub fn compile(patterns: &[String]) -> Self {
        Self {
            rules: patterns
                .iter()
                .map(|pattern| CompiledModelMapping::compile(pattern.clone()))
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn rule_is_valid(&self, index: usize) -> bool {
        self.rules
            .get(index)
            .is_some_and(CompiledModelMapping::is_valid)
    }

    pub fn rule_matches(&self, index: usize, model_name: &str) -> bool {
        self.rules
            .get(index)
            .is_some_and(|rule| rule.is_match(model_name))
    }

    pub fn matches_any(&self, model_name: &str) -> bool {
        self.rules.iter().any(|rule| rule.is_match(model_name))
    }

    pub fn matching_rule_indexes<'a>(
        &'a self,
        model_name: &'a str,
    ) -> impl Iterator<Item = usize> + 'a {
        self.rules
            .iter()
            .enumerate()
            .filter_map(move |(index, rule)| rule.is_match(model_name).then_some(index))
    }
}

struct CachedMappings {
    patterns: Vec<String>,
    compiled: Arc<CompiledModelMappings>,
    last_access: Instant,
}

#[derive(Default)]
struct ModelMappingCache {
    entries: HashMap<u64, Vec<CachedMappings>>,
    len: usize,
}

impl ModelMappingCache {
    fn get_or_compile(&mut self, patterns: &[String]) -> Arc<CompiledModelMappings> {
        let now = Instant::now();
        self.remove_expired(now);
        let hash = model_mapping_content_hash(patterns);
        if let Some(entry) = self
            .entries
            .get_mut(&hash)
            .and_then(|entries| entries.iter_mut().find(|entry| entry.patterns == patterns))
        {
            entry.last_access = now;
            MODEL_MAPPING_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            return Arc::clone(&entry.compiled);
        }

        MODEL_MAPPING_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
        if self.len >= MODEL_MAPPING_CACHE_CAPACITY {
            self.evict_oldest();
        }
        let compiled = Arc::new(CompiledModelMappings::compile(patterns));
        self.entries.entry(hash).or_default().push(CachedMappings {
            patterns: patterns.to_vec(),
            compiled: Arc::clone(&compiled),
            last_access: now,
        });
        self.len += 1;
        compiled
    }

    fn remove_expired(&mut self, now: Instant) {
        self.entries.retain(|_, entries| {
            entries
                .retain(|entry| now.duration_since(entry.last_access) <= MODEL_MAPPING_CACHE_TTL);
            !entries.is_empty()
        });
        self.len = self.entries.values().map(Vec::len).sum();
    }

    fn evict_oldest(&mut self) {
        let oldest = self
            .entries
            .iter()
            .flat_map(|(hash, entries)| {
                entries
                    .iter()
                    .enumerate()
                    .map(move |(index, entry)| (*hash, index, entry.last_access))
            })
            .min_by_key(|(_, _, last_access)| *last_access);
        let Some((hash, index, _)) = oldest else {
            return;
        };
        if let Some(entries) = self.entries.get_mut(&hash) {
            entries.swap_remove(index);
            self.len = self.len.saturating_sub(1);
            MODEL_MAPPING_CACHE_EVICTIONS.fetch_add(1, Ordering::Relaxed);
            if entries.is_empty() {
                self.entries.remove(&hash);
            }
        }
    }
}

fn model_mapping_content_hash(patterns: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    patterns.hash(&mut hasher);
    hasher.finish()
}

static MODEL_MAPPING_CACHE: LazyLock<Mutex<ModelMappingCache>> =
    LazyLock::new(|| Mutex::new(ModelMappingCache::default()));
static MODEL_MAPPING_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static MODEL_MAPPING_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static MODEL_MAPPING_CACHE_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static MODEL_MAPPING_RULE_COMPILES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMappingCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub rule_compiles: u64,
}

pub fn compiled_model_mappings(patterns: &[String]) -> Arc<CompiledModelMappings> {
    MODEL_MAPPING_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_compile(patterns)
}

pub fn model_mapping_cache_stats() -> ModelMappingCacheStats {
    ModelMappingCacheStats {
        hits: MODEL_MAPPING_CACHE_HITS.load(Ordering::Relaxed),
        misses: MODEL_MAPPING_CACHE_MISSES.load(Ordering::Relaxed),
        evictions: MODEL_MAPPING_CACHE_EVICTIONS.load(Ordering::Relaxed),
        rule_compiles: MODEL_MAPPING_RULE_COMPILES.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::{compiled_model_mappings, model_mapping_cache_stats};

    #[test]
    fn compiled_mappings_preserve_literal_regex_and_invalid_semantics() {
        let patterns = vec![
            "gpt-4o".to_string(),
            "gpt-5(?:\\.\\d+)?".to_string(),
            "([a-z".to_string(),
        ];
        let compiled = compiled_model_mappings(&patterns);

        assert!(compiled.rule_matches(0, "GPT-4O"));
        assert!(!compiled.rule_matches(0, "gpt-4o-mini"));
        assert!(compiled.rule_matches(1, "GPT-5.1"));
        assert!(!compiled.rule_is_valid(2));
        assert!(compiled.rule_matches(2, "([A-Z"));
    }

    #[test]
    fn cache_reuses_identical_rule_sets() {
        let patterns = vec![
            "cache-test-[0-9]+".to_string(),
            "literal-cache-test".to_string(),
        ];
        let before = model_mapping_cache_stats();
        let first = compiled_model_mappings(&patterns);
        let after_first = model_mapping_cache_stats();
        let second = compiled_model_mappings(&patterns);
        let after_second = model_mapping_cache_stats();

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert!(after_first.rule_compiles >= before.rule_compiles + 2);
        assert!(after_second.hits >= after_first.hits + 1);
    }
}
