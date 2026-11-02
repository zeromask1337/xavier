use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cost::round_usd;
use crate::errors::{AppError, Result};
use crate::types::CostSource;

pub const LEDGER_FILE: &str = "ledger.jsonl";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// RFC 3339 UTC timestamp.
    pub ts: String,
    /// Cost in USD, or null when the API did not report one.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub cost_source: Option<CostSource>,
    pub posts_fetched: i64,
    pub model: String,
    /// SHA-256 of the normalized query. The query text is only stored with
    /// `--ledger-query`.
    pub query_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default)]
    pub cached: bool,
}

pub struct RunRecord<'a> {
    pub query: &'a str,
    pub model: &'a str,
    pub cost_usd: Option<f64>,
    pub cost_source: Option<CostSource>,
    pub posts_fetched: i64,
    pub cached: bool,
    pub store_query: bool,
}

impl LedgerEntry {
    pub fn new(record: &RunRecord<'_>, now: DateTime<Utc>) -> Self {
        let (cost_usd, cost_source) = if record.cached {
            (Some(0.0), None)
        } else {
            (record.cost_usd.map(round_usd), record.cost_source)
        };
        Self {
            ts: format_timestamp(now),
            cost_usd,
            cost_source,
            posts_fetched: record.posts_fetched.max(0),
            model: record.model.to_string(),
            query_hash: query_hash(record.query),
            query: record.store_query.then(|| normalize_query(record.query)),
            cached: record.cached,
        }
    }

    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.ts)
            .ok()
            .map(|timestamp| timestamp.with_timezone(&Utc))
    }
}

/// SHA-256 of the whitespace-normalized query, hex encoded.
pub fn query_hash(query: &str) -> String {
    let digest = Sha256::digest(normalize_query(query).as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn format_timestamp(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Resolve the ledger file from `--ledger-dir`, `XAVIER_LEDGER_DIR`, or the
/// platform data directory.
pub fn resolve_path(explicit_dir: Option<&str>, env: &HashMap<String, String>) -> Result<PathBuf> {
    let dir = match explicit_dir.map(str::trim).filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => default_dir(env)?,
    };
    Ok(dir.join(LEDGER_FILE))
}

pub fn default_dir(env: &HashMap<String, String>) -> Result<PathBuf> {
    if let Some(dir) = env
        .get("XAVIER_LEDGER_DIR")
        .map(|dir| dir.trim())
        .filter(|dir| !dir.is_empty())
    {
        return Ok(PathBuf::from(dir));
    }

    if cfg!(target_os = "macos") {
        if let Some(home) = home_dir(env) {
            return Ok(home.join("Library/Application Support/xavier"));
        }
    } else if cfg!(target_os = "windows") {
        if let Some(local) = env
            .get("LOCALAPPDATA")
            .map(|dir| dir.trim())
            .filter(|dir| !dir.is_empty())
        {
            return Ok(PathBuf::from(local).join("xavier"));
        }
    } else {
        if let Some(data) = env
            .get("XDG_DATA_HOME")
            .map(|dir| dir.trim())
            .filter(|dir| !dir.is_empty())
        {
            return Ok(PathBuf::from(data).join("xavier"));
        }
        if let Some(home) = home_dir(env) {
            return Ok(home.join(".local/share/xavier"));
        }
    }

    Err(AppError::runtime_with_hint(
        "Could not resolve the ledger directory.",
        "Set XAVIER_LEDGER_DIR or pass --ledger-dir.",
    ))
}

fn home_dir(env: &HashMap<String, String>) -> Option<PathBuf> {
    env.get("HOME")
        .map(|home| home.trim())
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Append one entry as a single line. The write uses `O_APPEND` and one
/// `write_all`, so concurrent appends stay line-atomic on supported platforms.
pub fn append(path: &Path, entry: &LedgerEntry) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(entry).map_err(io::Error::other)?;
    let mut bytes = Vec::with_capacity(line.len() + 1);
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');

    let mut file = open_append(path)?;
    file.write_all(&bytes)
}

#[cfg(unix)]
fn open_append(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// Read every valid entry. Missing files are empty; corrupt lines are skipped.
pub fn read_entries(path: &Path) -> io::Result<Vec<LedgerEntry>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(parse_entries(&String::from_utf8_lossy(&bytes)))
}

pub fn parse_entries(text: &str) -> Vec<LedgerEntry> {
    text.lines().filter_map(parse_entry).collect()
}

fn parse_entry(line: &str) -> Option<LedgerEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let entry: LedgerEntry = serde_json::from_str(line).ok()?;
    if entry.query_hash.is_empty() || entry.timestamp().is_none() {
        return None;
    }
    if entry
        .cost_usd
        .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
    {
        return None;
    }
    if entry.posts_fetched < 0 {
        return None;
    }
    Some(entry)
}

pub fn window_start(now: DateTime<Utc>, seconds: u64) -> DateTime<Utc> {
    let Ok(seconds) = i64::try_from(seconds) else {
        return DateTime::<Utc>::MIN_UTC;
    };
    Duration::try_seconds(seconds)
        .and_then(|duration| now.checked_sub_signed(duration))
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}

pub fn window_spend(entries: &[LedgerEntry], since: DateTime<Utc>) -> f64 {
    round_usd(
        entries
            .iter()
            .filter(|entry| entry.timestamp().is_some_and(|ts| ts >= since))
            .filter_map(|entry| entry.cost_usd)
            .sum(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetCheck {
    pub spent_usd: f64,
    pub estimate_usd: f64,
    pub budget_usd: f64,
    pub window_seconds: u64,
}

impl BudgetCheck {
    pub fn projected_usd(&self) -> f64 {
        round_usd(self.spent_usd + self.estimate_usd)
    }

    pub fn exceeded(&self) -> bool {
        self.projected_usd() > round_usd(self.budget_usd)
    }
}

pub fn check_budget(
    entries: &[LedgerEntry],
    now: DateTime<Utc>,
    window_seconds: u64,
    budget_usd: f64,
    estimate_usd: f64,
) -> BudgetCheck {
    let since = window_start(now, window_seconds);
    BudgetCheck {
        spent_usd: window_spend(entries, since),
        estimate_usd: round_usd(estimate_usd),
        budget_usd: round_usd(budget_usd),
        window_seconds,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayStats {
    pub day: String,
    pub runs: u64,
    pub cost_usd: f64,
    pub cache_hits: u64,
    pub posts_fetched: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelStats {
    pub model: String,
    pub runs: u64,
    pub cost_usd: f64,
    pub cache_hits: u64,
    pub posts_fetched: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LedgerStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    pub runs: u64,
    pub cost_usd: f64,
    pub cache_hits: u64,
    pub posts_fetched: i64,
    pub unknown_cost_runs: u64,
    pub by_day: Vec<DayStats>,
    pub by_model: Vec<ModelStats>,
}

#[derive(Default)]
struct Accumulator {
    runs: u64,
    cost_usd: f64,
    cache_hits: u64,
    posts_fetched: i64,
    unknown_cost_runs: u64,
}

impl Accumulator {
    fn add(&mut self, entry: &LedgerEntry) {
        self.runs += 1;
        match entry.cost_usd {
            Some(cost) => self.cost_usd += cost,
            None => self.unknown_cost_runs += 1,
        }
        if entry.cached {
            self.cache_hits += 1;
        }
        self.posts_fetched += entry.posts_fetched.max(0);
    }
}

pub fn aggregate(entries: &[LedgerEntry], since: Option<DateTime<Utc>>) -> LedgerStats {
    let mut total = Accumulator::default();
    let mut by_day: BTreeMap<String, Accumulator> = BTreeMap::new();
    let mut by_model: BTreeMap<String, Accumulator> = BTreeMap::new();

    for entry in entries {
        let Some(timestamp) = entry.timestamp() else {
            continue;
        };
        if since.is_some_and(|since| timestamp < since) {
            continue;
        }
        total.add(entry);
        by_day
            .entry(timestamp.format("%Y-%m-%d").to_string())
            .or_default()
            .add(entry);
        by_model.entry(entry.model.clone()).or_default().add(entry);
    }

    let mut by_model: Vec<ModelStats> = by_model
        .into_iter()
        .map(|(model, accumulator)| ModelStats {
            model,
            runs: accumulator.runs,
            cost_usd: round_usd(accumulator.cost_usd),
            cache_hits: accumulator.cache_hits,
            posts_fetched: accumulator.posts_fetched,
        })
        .collect();
    by_model.sort_by(|left, right| {
        right
            .cost_usd
            .partial_cmp(&left.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.model.cmp(&right.model))
    });

    LedgerStats {
        since: since.map(format_timestamp),
        runs: total.runs,
        cost_usd: round_usd(total.cost_usd),
        cache_hits: total.cache_hits,
        posts_fetched: total.posts_fetched,
        unknown_cost_runs: total.unknown_cost_runs,
        by_day: by_day
            .into_iter()
            .map(|(day, accumulator)| DayStats {
                day,
                runs: accumulator.runs,
                cost_usd: round_usd(accumulator.cost_usd),
                cache_hits: accumulator.cache_hits,
                posts_fetched: accumulator.posts_fetched,
            })
            .collect(),
        by_model,
    }
}

/// A tiny explicit directory guard keeps ledger tests isolated without a
/// temporary-directory dependency.
#[cfg(test)]
struct TempDir {
    path: PathBuf,
}

#[cfg(test)]
impl TempDir {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "xavier-ledger-unit-{}-{}-{tag}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

#[cfg(test)]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap()
    }

    fn entry(ts: &str, cost: Option<f64>, model: &str, cached: bool) -> LedgerEntry {
        LedgerEntry {
            ts: ts.to_string(),
            cost_usd: cost,
            cost_source: cost.map(|_| CostSource::Api),
            posts_fetched: 5,
            model: model.to_string(),
            query_hash: query_hash("grok"),
            query: None,
            cached,
        }
    }

    #[test]
    fn hashes_queries_without_storing_text() {
        let first = query_hash("  grok  4.7 ");
        let second = query_hash("grok 4.7");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, query_hash("grok 4.6"));
        assert!(!first.contains("grok"));
    }

    #[test]
    fn builds_entries_with_and_without_query_text() {
        let record = RunRecord {
            query: "grok 4.7",
            model: "grok-4.7",
            cost_usd: Some(0.0012345),
            cost_source: Some(CostSource::Api),
            posts_fetched: 3,
            cached: false,
            store_query: false,
        };
        let hidden = LedgerEntry::new(&record, now());
        assert_eq!(hidden.query, None);
        assert_eq!(hidden.cost_usd, Some(0.001235));
        assert_eq!(hidden.query_hash, query_hash("grok 4.7"));

        let shown = LedgerEntry::new(
            &RunRecord {
                store_query: true,
                ..record
            },
            now(),
        );
        assert_eq!(shown.query.as_deref(), Some("grok 4.7"));
    }

    #[test]
    fn cached_runs_record_zero_cost() {
        let cached = LedgerEntry::new(
            &RunRecord {
                query: "grok",
                model: "grok-4.7",
                cost_usd: Some(0.5),
                cost_source: Some(CostSource::Api),
                posts_fetched: 10,
                cached: true,
                store_query: false,
            },
            now(),
        );
        assert_eq!(cached.cost_usd, Some(0.0));
        assert_eq!(cached.cost_source, None);
        assert!(cached.cached);
    }

    #[test]
    fn appends_and_reads_round_trip() {
        let temp = TempDir::new("round-trip");
        let path = temp.path.join(LEDGER_FILE);
        let first = entry("2026-11-01T10:00:00.000Z", Some(0.001), "grok-4.7", false);
        let second = entry("2026-11-02T11:00:00.000Z", Some(0.002), "grok-4.6", true);

        append(&path, &first).unwrap();
        append(&path, &second).unwrap();

        let entries = read_entries(&path).unwrap();
        assert_eq!(entries, vec![first, second]);
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn missing_ledger_reads_as_empty() {
        let temp = TempDir::new("missing");
        assert!(
            read_entries(&temp.path.join(LEDGER_FILE))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn skips_corrupt_lines() {
        let text = [
            r#"{"ts":"2026-11-01T10:00:00.000Z","cost_usd":0.1,"cost_source":"api","posts_fetched":5,"model":"grok-4.7","query_hash":"abc","cached":false}"#,
            "not json",
            "",
            r#"{"ts":"not-a-date","cost_usd":0.1,"cost_source":"api","posts_fetched":5,"model":"grok-4.7","query_hash":"abc","cached":false}"#,
            r#"{"ts":"2026-11-02T10:00:00.000Z","cost_usd":-1.0,"cost_source":"api","posts_fetched":5,"model":"grok-4.7","query_hash":"abc","cached":false}"#,
            r#"{"ts":"2026-11-02T10:00:00.000Z","cost_usd":0.2,"posts_fetched":1,"model":"grok-4.6","query_hash":"def"}"#,
        ]
        .join("\n");

        let entries = parse_entries(&text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].cost_usd, Some(0.1));
        assert_eq!(entries[1].cost_usd, Some(0.2));
        assert!(!entries[1].cached);
    }

    #[test]
    fn aggregates_totals_by_day_and_model() {
        let entries = vec![
            entry("2026-11-01T10:00:00.000Z", Some(0.1), "grok-4.7", false),
            entry("2026-11-01T11:00:00.000Z", Some(0.2), "grok-4.7", true),
            entry("2026-11-02T09:00:00.000Z", Some(0.3), "grok-4.6", false),
            entry("2026-11-02T10:00:00.000Z", None, "grok-4.6", false),
        ];

        let stats = aggregate(&entries, None);
        assert_eq!(stats.runs, 4);
        assert_eq!(stats.cost_usd, 0.6);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.posts_fetched, 20);
        assert_eq!(stats.unknown_cost_runs, 1);
        assert_eq!(stats.by_day.len(), 2);
        assert_eq!(stats.by_day[0].day, "2026-11-01");
        assert_eq!(stats.by_day[0].cost_usd, 0.3);
        assert_eq!(stats.by_day[1].runs, 2);
        assert_eq!(stats.by_model.len(), 2);
        assert_eq!(stats.by_model[0].model, "grok-4.6");
        assert_eq!(stats.by_model[0].cost_usd, 0.3);
        assert_eq!(stats.by_model[1].cache_hits, 1);
    }

    #[test]
    fn filters_stats_by_since() {
        let entries = vec![
            entry("2026-10-01T10:00:00.000Z", Some(0.1), "grok-4.7", false),
            entry("2026-11-01T10:00:00.000Z", Some(0.2), "grok-4.7", false),
        ];
        let stats = aggregate(&entries, Some(window_start(now(), 30 * 86_400)));
        assert_eq!(stats.runs, 1);
        assert_eq!(stats.cost_usd, 0.2);
        assert_eq!(stats.since.as_deref(), Some("2026-10-03T12:00:00.000Z"));
    }

    #[test]
    fn budget_windows_and_estimates_gate_runs() {
        let entries = vec![
            entry("2026-10-01T10:00:00.000Z", Some(0.5), "grok-4.7", false),
            entry("2026-11-01T10:00:00.000Z", Some(0.2), "grok-4.7", false),
        ];

        let check = check_budget(&entries, now(), 30 * 86_400, 0.5, 0.3);
        assert_eq!(check.spent_usd, 0.2);
        assert_eq!(check.estimate_usd, 0.3);
        assert_eq!(check.projected_usd(), 0.5);
        assert!(!check.exceeded());

        let refused = check_budget(&entries, now(), 30 * 86_400, 0.49, 0.3);
        assert!(refused.exceeded());
    }

    #[test]
    fn resolves_explicit_and_platform_paths() {
        let env = HashMap::new();
        let explicit = resolve_path(Some("/tmp/xavier-ledger"), &env).unwrap();
        assert_eq!(explicit, Path::new("/tmp/xavier-ledger").join(LEDGER_FILE));

        let env = HashMap::from([("XAVIER_LEDGER_DIR".to_string(), "/tmp/from-env".to_string())]);
        assert_eq!(
            resolve_path(None, &env).unwrap(),
            Path::new("/tmp/from-env").join(LEDGER_FILE)
        );

        let home = "/tmp/xavier-home";
        let env = HashMap::from([("HOME".to_string(), home.to_string())]);
        let default = default_dir(&env).unwrap();
        if cfg!(target_os = "macos") {
            assert_eq!(
                default,
                Path::new(home).join("Library/Application Support/xavier")
            );
        } else {
            assert_eq!(default, Path::new(home).join(".local/share/xavier"));
        }
    }

    #[test]
    fn json_lines_keep_the_documented_shape() {
        let entry = entry("2026-11-02T12:00:00.000Z", Some(0.001), "grok-4.7", false);
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&entry).unwrap())
                .expect("entry serializes");
        assert_eq!(value["ts"], "2026-11-02T12:00:00.000Z");
        assert_eq!(value["cost_usd"], 0.001);
        assert_eq!(value["cost_source"], "api");
        assert_eq!(value["posts_fetched"], 5);
        assert_eq!(value["model"], "grok-4.7");
        assert_eq!(value["query_hash"], query_hash("grok"));
        assert_eq!(value["cached"], false);
        assert!(value.get("query").is_none());
    }
}
