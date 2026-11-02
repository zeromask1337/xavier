use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::args::{CacheMode, Options};
use crate::errors::{AppError, Result};
use crate::types::SearchResult;

/// Bump when the cache entry shape or key inputs change.
pub const CACHE_FORMAT_VERSION: u32 = 1;

const CACHE_FILE_EXTENSION: &str = "json";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub version: u32,
    /// Hash of the normalized request; also used as the file name.
    pub key: String,
    /// Unix timestamp in seconds.
    pub created_at: i64,
    /// Raw xAI Responses API payload, for `--raw` and diagnostics.
    pub raw: Value,
    /// Fully rendered result, so replay works without a query or API key.
    pub result: SearchResult,
}

impl CacheEntry {
    pub fn new(key: String, result: &SearchResult, raw: Value, created_at: i64) -> Self {
        Self {
            version: CACHE_FORMAT_VERSION,
            key,
            created_at,
            raw,
            result: result.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedCache {
    pub entry: CacheEntry,
    pub age_seconds: u64,
}

pub struct CacheStore {
    dir: PathBuf,
    ttl_seconds: u64,
}

impl CacheStore {
    pub fn new(dir: PathBuf, ttl_seconds: u64) -> Self {
        Self { dir, ttl_seconds }
    }

    /// Build a store from CLI flags and environment. Returns `None` when caching
    /// is off or no cache directory can be resolved.
    pub fn from_options(options: &Options, env: &HashMap<String, String>) -> Option<Self> {
        if options.cache == CacheMode::Off {
            return None;
        }
        let dir = resolve_cache_dir(options.cache_dir.as_deref(), env)?;
        Some(Self::new(dir, options.cache_ttl_seconds))
    }

    pub fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.{CACHE_FILE_EXTENSION}"))
    }

    /// Read a matching, unexpired entry. Corrupt, stale, or expired entries are
    /// removed and ignored; caching must never fail a search.
    pub fn load(&self, key: &str, now: i64) -> Option<LoadedCache> {
        let path = self.path(key);
        let text = fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = match serde_json::from_str(&text) {
            Ok(entry) => entry,
            Err(_) => {
                remove_quiet(&path);
                return None;
            }
        };

        if entry.version != CACHE_FORMAT_VERSION || entry.key != key {
            remove_quiet(&path);
            return None;
        }

        let age = now.saturating_sub(entry.created_at);
        if age < 0 || age as u64 >= self.ttl_seconds {
            remove_quiet(&path);
            return None;
        }

        Some(LoadedCache {
            entry,
            age_seconds: age as u64,
        })
    }

    /// Write an entry atomically with `0600` permissions on Unix.
    pub fn store(&self, entry: &CacheEntry) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec_pretty(entry).map_err(io::Error::other)?;
        let target = self.path(&entry.key);
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp = self.dir.join(format!(
            ".{}.{}.{}.tmp",
            entry.key,
            std::process::id(),
            unique
        ));

        let result = (|| -> io::Result<()> {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);

            #[cfg(windows)]
            {
                if target.exists() {
                    fs::remove_file(&target)?;
                }
            }
            fs::rename(&temp, &target)
        })();

        if result.is_err() {
            remove_quiet(&temp);
        }
        result
    }
}

/// Resolve the cache directory: explicit flag, then `XAVIER_CACHE_DIR`, then
/// the platform cache directory.
pub fn resolve_cache_dir(
    explicit: Option<&Path>,
    env: &HashMap<String, String>,
) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    if let Some(value) = env
        .get("XAVIER_CACHE_DIR")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Some(PathBuf::from(value));
    }
    platform_cache_dir(env)
}

#[cfg(target_os = "macos")]
fn platform_cache_dir(env: &HashMap<String, String>) -> Option<PathBuf> {
    home_dir(env).map(|home| home.join("Library/Caches/xavier"))
}

#[cfg(windows)]
fn platform_cache_dir(env: &HashMap<String, String>) -> Option<PathBuf> {
    env_dir(env, "LOCALAPPDATA").map(|base| base.join("xavier"))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_cache_dir(env: &HashMap<String, String>) -> Option<PathBuf> {
    env_dir(env, "XDG_CACHE_HOME")
        .or_else(|| home_dir(env).map(|home| home.join(".cache")))
        .map(|base| base.join("xavier"))
}

#[cfg(not(windows))]
fn home_dir(env: &HashMap<String, String>) -> Option<PathBuf> {
    env_dir(env, "HOME")
}

fn env_dir(env: &HashMap<String, String>, name: &str) -> Option<PathBuf> {
    env.get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Stable cache key for the normalized request plus backend identity.
pub fn cache_key(options: &Options, base_url: &str) -> String {
    let mut handles = options.handles.clone();
    handles.sort();
    let mut exclude_handles = options.exclude_handles.clone();
    exclude_handles.sort();

    let canonical = json!({
        "cache_version": CACHE_FORMAT_VERSION,
        "prompt_version": crate::xai::PROMPT_VERSION,
        "base_url": base_url,
        "query": options.query,
        "model": options.model,
        "effort": options.effort.as_str(),
        "max_turns": options.max_turns,
        "limit": options.limit,
        "from": options.from,
        "to": options.to,
        "handles": handles,
        "exclude_handles": exclude_handles,
        "sort": options.sort.map(|sort| sort.as_str()),
        "images": options.images,
        "videos": options.videos,
    });

    let serialized = serde_json::to_vec(&canonical).expect("cache key is serializable");
    hex(&Sha256::digest(serialized))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplaySource {
    Cache(Box<CacheEntry>),
    Raw(Value),
}

/// Read a raw xAI response or a xavier cache entry from disk.
pub fn read_replay(path: &Path) -> Result<ReplaySource> {
    let text = fs::read_to_string(path).map_err(|error| {
        AppError::runtime_with_hint(
            format!("Could not read replay file {}: {error}", path.display()),
            "Check the path and try again.",
        )
    })?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        AppError::runtime_with_hint(
            format!("Replay file {} is not valid JSON: {error}", path.display()),
            "Pass a raw xAI response or a xavier cache entry.",
        )
    })?;

    let looks_like_entry = value.get("version").is_some()
        && value.get("key").is_some()
        && value.get("raw").is_some()
        && value.get("result").is_some();
    if looks_like_entry {
        let entry: CacheEntry = serde_json::from_value(value)
            .map_err(|error| AppError::runtime(format!("Could not parse cache entry: {error}")))?;
        if entry.version != CACHE_FORMAT_VERSION {
            return Err(AppError::runtime(format!(
                "Unsupported cache entry version {} (expected {}).",
                entry.version, CACHE_FORMAT_VERSION
            )));
        }
        return Ok(ReplaySource::Cache(Box::new(entry)));
    }

    Ok(ReplaySource::Raw(value))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn remove_quiet(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{Options, OutputFormat};
    use crate::types::{Effort, SortMode};
    use crate::xai::build_result;
    use std::sync::atomic::AtomicUsize;

    const FIXTURE: &str = include_str!("../tests/fixtures/response.json");

    fn options() -> Options {
        Options {
            query: "grok 4.7".to_string(),
            format: OutputFormat::Md,
            model: "grok-4.7".to_string(),
            handles: Vec::new(),
            exclude_handles: Vec::new(),
            from: None,
            to: None,
            limit: 20,
            max_turns: 2,
            effort: Effort::Low,
            sort: None,
            images: false,
            videos: false,
            timeout_ms: 180_000,
            dry_run: false,
            verbose: false,
            ledger: crate::args::LedgerMode::On,
            ledger_dir: None,
            ledger_query: false,
            budget_usd: None,
            budget_window_seconds: crate::args::DEFAULT_BUDGET_WINDOW_SECONDS,
            budget_multiplier: crate::cost::DEFAULT_BUDGET_MULTIPLIER,
            cache: CacheMode::On,
            cache_ttl_seconds: 900,
            cache_dir: None,
            replay: None,
        }
    }

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("fixture is valid JSON")
    }

    fn result() -> SearchResult {
        build_result(&fixture(), &options(), 1234).expect("fixture builds a result")
    }

    fn temp_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "xavier-cache-test-{}-{}-{}",
            std::process::id(),
            label,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn entry(key: &str) -> CacheEntry {
        CacheEntry::new(key.to_string(), &result(), fixture(), 1_000)
    }

    #[test]
    fn key_is_stable_and_sensitive_to_request_inputs() {
        let base = cache_key(&options(), "https://api.x.ai/v1");
        assert_eq!(base, cache_key(&options(), "https://api.x.ai/v1"));

        let variants = [
            Options {
                query: "grok 4.6".to_string(),
                ..options()
            },
            Options {
                model: "grok-4.6".to_string(),
                ..options()
            },
            Options {
                effort: Effort::High,
                ..options()
            },
            Options {
                max_turns: 5,
                ..options()
            },
            Options {
                limit: 5,
                ..options()
            },
            Options {
                from: Some("2026-10-18".to_string()),
                ..options()
            },
            Options {
                handles: vec!["alice".to_string()],
                ..options()
            },
            Options {
                exclude_handles: vec!["bob".to_string()],
                ..options()
            },
            Options {
                sort: Some(SortMode::Latest),
                ..options()
            },
            Options {
                images: true,
                ..options()
            },
            Options {
                videos: true,
                ..options()
            },
        ];

        for variant in variants {
            assert_ne!(base, cache_key(&variant, "https://api.x.ai/v1"));
        }
        assert_ne!(base, cache_key(&options(), "https://example.test/v1"));
    }

    #[test]
    fn key_ignores_handle_order() {
        let mut first = options();
        first.handles = vec!["alice".to_string(), "bob".to_string()];
        let mut second = options();
        second.handles = vec!["bob".to_string(), "alice".to_string()];
        assert_eq!(
            cache_key(&first, "https://api.x.ai/v1"),
            cache_key(&second, "https://api.x.ai/v1")
        );
    }

    #[test]
    fn stores_and_loads_entries() {
        let dir = temp_dir("round-trip");
        let store = CacheStore::new(dir.clone(), 900);
        let key = cache_key(&options(), "https://api.x.ai/v1");
        let entry = entry(&key);

        store.store(&entry).expect("store entry");
        let loaded = store.load(&key, 1_010).expect("load entry");
        assert_eq!(loaded.age_seconds, 10);
        assert_eq!(loaded.entry.result.posts.len(), 2);
        assert_eq!(loaded.entry.raw["id"], "resp_test_123");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_private_files_atomically() {
        let dir = temp_dir("permissions");
        let store = CacheStore::new(dir.clone(), 900);
        let key = cache_key(&options(), "https://api.x.ai/v1");
        store.store(&entry(&key)).expect("store entry");

        let path = store.path(&key);
        assert!(path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|item| item.ok())
            .filter(|item| item.path().extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert!(leftovers.is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn removes_expired_entries() {
        let dir = temp_dir("expired");
        let store = CacheStore::new(dir.clone(), 900);
        let key = cache_key(&options(), "https://api.x.ai/v1");
        store.store(&entry(&key)).expect("store entry");

        assert!(store.load(&key, 1_900).is_none());
        assert!(!store.path(&key).exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ignores_and_removes_corrupt_entries() {
        let dir = temp_dir("corrupt");
        let store = CacheStore::new(dir.clone(), 900);
        let key = cache_key(&options(), "https://api.x.ai/v1");
        fs::write(store.path(&key), "{not json").expect("write corrupt entry");

        assert!(store.load(&key, 1_000).is_none());
        assert!(!store.path(&key).exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ignores_entries_for_other_keys() {
        let dir = temp_dir("wrong-key");
        let store = CacheStore::new(dir.clone(), 900);
        let key = cache_key(&options(), "https://api.x.ai/v1");
        let other = cache_key(&options(), "https://example.test/v1");
        fs::write(
            store.path(&key),
            serde_json::to_string(&entry(&other)).unwrap(),
        )
        .unwrap();

        assert!(store.load(&key, 1_010).is_none());
        assert!(!store.path(&key).exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_cache_dir_precedence() {
        let env = HashMap::from([
            ("XAVIER_CACHE_DIR".to_string(), "/tmp/from-env".to_string()),
            ("HOME".to_string(), "/home/tester".to_string()),
        ]);
        assert_eq!(
            resolve_cache_dir(Some(Path::new("/tmp/explicit")), &env),
            Some(PathBuf::from("/tmp/explicit"))
        );
        assert_eq!(
            resolve_cache_dir(None, &env),
            Some(PathBuf::from("/tmp/from-env"))
        );

        let empty = HashMap::new();
        assert_eq!(resolve_cache_dir(None, &empty), None);
    }

    #[test]
    fn reads_raw_responses_and_cache_entries_for_replay() {
        let dir = temp_dir("replay");
        let raw_path = dir.join("response.json");
        fs::write(&raw_path, FIXTURE).unwrap();
        assert!(matches!(
            read_replay(&raw_path).unwrap(),
            ReplaySource::Raw(_)
        ));

        let key = cache_key(&options(), "https://api.x.ai/v1");
        let entry_path = dir.join("entry.json");
        fs::write(&entry_path, serde_json::to_string(&entry(&key)).unwrap()).unwrap();
        match read_replay(&entry_path).unwrap() {
            ReplaySource::Cache(entry) => assert_eq!(entry.result.posts.len(), 2),
            other => panic!("expected a cache entry, got {other:?}"),
        }

        let bad_path = dir.join("bad.json");
        fs::write(&bad_path, "not json").unwrap();
        assert!(read_replay(&bad_path).is_err());

        let _ = fs::remove_dir_all(dir);
    }
}
