use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use xavier::ledger::{self, LedgerEntry, RunRecord};
use xavier::types::CostSource;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "xavier-ledger-integration-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn cached_runs_record_zero_cost_and_count_as_cache_hits() {
    let temp = TempDir::new("cached");
    let path = temp.path.join(ledger::LEDGER_FILE);
    let now = Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap();

    let entry = LedgerEntry::new(
        &RunRecord {
            query: "grok 4.7",
            model: "grok-4.7",
            cost_usd: Some(0.42),
            cost_source: Some(CostSource::Api),
            posts_fetched: 10,
            cached: true,
            store_query: false,
        },
        now,
    );
    ledger::append(&path, &entry).expect("append cached entry");

    let entries = ledger::read_entries(&path).expect("read ledger");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].cost_usd, Some(0.0));
    assert!(entries[0].cached);
    assert!(entries[0].query.is_none());

    let stats = ledger::aggregate(&entries, None);
    assert_eq!(stats.runs, 1);
    assert_eq!(stats.cost_usd, 0.0);
    assert_eq!(stats.cache_hits, 1);
}

#[test]
fn budget_window_uses_ledger_spend_plus_the_soft_estimate() {
    let temp = TempDir::new("budget");
    let path = temp.path.join(ledger::LEDGER_FILE);
    let now = Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap();

    for (index, cost) in [0.2_f64, 0.1_f64].into_iter().enumerate() {
        let entry = LedgerEntry::new(
            &RunRecord {
                query: "grok 4.7",
                model: "grok-4.7",
                cost_usd: Some(cost),
                cost_source: Some(CostSource::Api),
                posts_fetched: 5,
                cached: false,
                store_query: false,
            },
            now - chrono::Duration::days(index as i64 + 1),
        );
        ledger::append(&path, &entry).expect("append entry");
    }

    let entries = ledger::read_entries(&path).expect("read ledger");
    let check = ledger::check_budget(&entries, now, 30 * 86_400, 0.4, 0.2);
    assert_eq!(check.spent_usd, 0.3);
    assert_eq!(check.projected_usd(), 0.5);
    assert!(check.exceeded());

    let allowed = ledger::check_budget(&entries, now, 30 * 86_400, 0.5, 0.2);
    assert!(!allowed.exceeded());
}
