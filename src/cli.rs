use std::collections::HashMap;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::args::{
    CacheMode, LedgerMode, Options, OutputFormat, ParsedCli, StatsOptions, help_text, parse_cli,
};
use crate::cache::{CacheEntry, CacheStore, LoadedCache, ReplaySource, cache_key, read_replay};
use crate::cost::{estimate_run_cost, format_usd, round_usd};
use crate::dates::format_duration_seconds;
use crate::errors::AppError;
use crate::ledger::{self, LedgerEntry, RunRecord};
use crate::output::{render_json, render_jsonl, render_markdown, render_stats, render_stats_json};
use crate::types::{CacheMeta, SearchResult};
use crate::xai::{
    build_request_body, build_result, request_x_posts, resolve_base_url, summarize_raw,
};

pub fn run(argv: Vec<String>) -> i32 {
    let env: HashMap<String, String> = std::env::vars().collect();
    let now = Utc::now();

    let parsed = match parse_cli(&argv, &env, now) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprint!("xavier: {}\n\n{}", error.message(), help_text());
            return error.exit_code();
        }
    };

    let mut options = match parsed {
        ParsedCli::Help => {
            print!("{}", help_text());
            return 0;
        }
        ParsedCli::Version => {
            println!("xavier {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        ParsedCli::Stats(options) => return run_stats(options, &env, now),
        ParsedCli::Run(options) => *options,
    };

    let base_url = resolve_base_url(env.get("XAI_BASE_URL").map(String::as_str));

    if let Some(path) = options.replay.clone() {
        return run_replay(&path, &options);
    }

    let mut query = options.query.clone();
    if query == "-" || (query.is_empty() && !io::stdin().is_terminal()) {
        let piped = read_stdin();
        if query == "-" || !piped.is_empty() {
            query = piped;
        }
    }

    query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        eprint!(
            "xavier: missing query.\n\nUsage: xavier [search] <query> [options]\n       echo \"<query>\" | xavier -\n"
        );
        return 2;
    }
    options.query = query;

    if options.dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&build_request_body(&options))
                .expect("request body is serializable")
        );
        return 0;
    }

    let ledger_path = if options.ledger == LedgerMode::On {
        match ledger::resolve_path(options.ledger_dir.as_deref(), &env) {
            Ok(path) => Some(path),
            Err(error) => {
                report(&error);
                return error.exit_code();
            }
        }
    } else {
        None
    };

    let store = CacheStore::from_options(&options, &env);
    let key = cache_key(&options, &base_url);

    if let Some(store) = &store
        && options.cache != CacheMode::Refresh
        && let Some(loaded) = store.load(&key, now.timestamp())
    {
        return render_cached(loaded, &options, ledger_path.as_deref());
    }

    if let (Some(path), Some(budget)) = (&ledger_path, options.budget_usd) {
        let entries = match ledger::read_entries(path) {
            Ok(entries) => entries,
            Err(error) => {
                let error = AppError::runtime_with_hint(
                    format!("Could not read ledger {}: {error}", path.display()),
                    "Check --ledger-dir permissions or disable the ledger with --ledger off.",
                );
                report(&error);
                return error.exit_code();
            }
        };
        let estimate = estimate_run_cost(options.limit, options.budget_multiplier);
        let check = ledger::check_budget(
            &entries,
            now,
            options.budget_window_seconds,
            budget,
            estimate,
        );
        if check.exceeded() {
            let error = AppError::budget(
                format!(
                    "budget exceeded: {} spent in the last {} plus a {} estimate exceeds {}.",
                    format_usd(Some(check.spent_usd)),
                    format_duration_seconds(check.window_seconds),
                    format_usd(Some(check.estimate_usd)),
                    format_usd(Some(check.budget_usd))
                ),
                "Raise --budget-usd or inspect spend with `xavier stats`.",
            );
            report(&error);
            return error.exit_code();
        }
    }

    let api_key = env.get("XAI_API_KEY").map(|value| value.trim().to_string());
    let Some(api_key) = api_key.filter(|value| !value.is_empty()) else {
        eprint!(
            "xavier: XAI_API_KEY is not set.\n  Run with: doppler run -- xavier \"your query\"\n  Or: export XAI_API_KEY=...\n"
        );
        return 2;
    };

    let raw = match request_x_posts(&options, &api_key, Some(&base_url)) {
        Ok(raw) => raw,
        Err(error) => {
            report(&error);
            return error.exit_code();
        }
    };

    if options.format == OutputFormat::Raw {
        println!(
            "{}",
            serde_json::to_string_pretty(&raw.raw).expect("raw payload is serializable")
        );
        if options.verbose {
            if let Some(store) = &store {
                eprintln!("xavier: cache=miss key={}", store.path(&key).display());
            }
            eprintln!("xavier: raw response received in {}ms", raw.elapsed_ms);
        }
        if let Some(path) = &ledger_path {
            let summary = summarize_raw(&raw.raw, &options.model);
            let entry = LedgerEntry::new(
                &RunRecord {
                    query: &options.query,
                    model: &summary.model,
                    cost_usd: summary.cost_usd,
                    cost_source: summary.cost_source,
                    posts_fetched: summary.posts_fetched,
                    cached: false,
                    store_query: options.ledger_query,
                },
                Utc::now(),
            );
            record_run(path, &entry, &options);
        }
        return 0;
    }

    let mut result = match build_result(&raw.raw, &options, raw.elapsed_ms) {
        Ok(result) => result,
        Err(error) => {
            report(&error);
            return error.exit_code();
        }
    };

    if let Some(store) = &store {
        let entry = CacheEntry::new(
            key.clone(),
            &result,
            raw.raw.clone(),
            Utc::now().timestamp(),
        );
        match store.store(&entry) {
            Ok(()) => {
                if options.verbose {
                    eprintln!(
                        "xavier: cache=stored key={} path={}",
                        key,
                        store.path(&key).display()
                    );
                }
            }
            Err(error) => {
                if options.verbose {
                    eprintln!("xavier: cache=store-failed {error}");
                }
            }
        }
        result.meta.cache = Some(CacheMeta {
            hit: false,
            age_seconds: None,
            saved_usd: None,
        });
    }

    print_result(&result, options.format);
    print_diagnostics(&result, &options, None);

    if let Some(path) = &ledger_path {
        let entry = LedgerEntry::new(
            &RunRecord {
                query: &result.query,
                model: &result.meta.model,
                cost_usd: result.meta.usage.cost_usd,
                cost_source: result.meta.usage.cost_source,
                posts_fetched: result.meta.usage.x_posts_fetched,
                cached: false,
                store_query: options.ledger_query,
            },
            Utc::now(),
        );
        record_run(path, &entry, &options);
    }

    0
}

fn render_cached(loaded: LoadedCache, options: &Options, ledger_path: Option<&Path>) -> i32 {
    let saved_usd = loaded.entry.result.meta.usage.cost_usd;

    if options.format == OutputFormat::Raw {
        println!(
            "{}",
            serde_json::to_string_pretty(&loaded.entry.raw).expect("raw payload is serializable")
        );
        if options.verbose {
            eprintln!(
                "xavier: cache=hit age={}s saved={}",
                loaded.age_seconds,
                format_usd(saved_usd)
            );
        }
        if let Some(path) = ledger_path {
            let entry = LedgerEntry::new(
                &RunRecord {
                    query: &loaded.entry.result.query,
                    model: &loaded.entry.result.meta.model,
                    cost_usd: saved_usd,
                    cost_source: loaded.entry.result.meta.usage.cost_source,
                    posts_fetched: loaded.entry.result.meta.usage.x_posts_fetched,
                    cached: true,
                    store_query: options.ledger_query,
                },
                Utc::now(),
            );
            record_run(path, &entry, options);
        }
        return 0;
    }

    let mut result = loaded.entry.result.clone();
    result.meta.cache = Some(CacheMeta {
        hit: true,
        age_seconds: Some(loaded.age_seconds),
        saved_usd,
    });

    print_result(&result, options.format);
    print_diagnostics(
        &result,
        options,
        Some(format!(
            "cache=hit age={}s saved={}",
            loaded.age_seconds,
            format_usd(saved_usd)
        )),
    );

    if let Some(path) = ledger_path {
        let entry = LedgerEntry::new(
            &RunRecord {
                query: &result.query,
                model: &result.meta.model,
                cost_usd: result.meta.usage.cost_usd,
                cost_source: result.meta.usage.cost_source,
                posts_fetched: result.meta.usage.x_posts_fetched,
                cached: true,
                store_query: options.ledger_query,
            },
            Utc::now(),
        );
        record_run(path, &entry, options);
    }

    0
}

fn run_stats(options: StatsOptions, env: &HashMap<String, String>, now: DateTime<Utc>) -> i32 {
    let path = match ledger::resolve_path(options.ledger_dir.as_deref(), env) {
        Ok(path) => path,
        Err(error) => {
            report(&error);
            return error.exit_code();
        }
    };
    let entries = match ledger::read_entries(&path) {
        Ok(entries) => entries,
        Err(error) => {
            let error = AppError::runtime_with_hint(
                format!("Could not read ledger {}: {error}", path.display()),
                "Check --ledger-dir permissions.",
            );
            report(&error);
            return error.exit_code();
        }
    };
    let since = options
        .since_seconds
        .map(|seconds| ledger::window_start(now, seconds));
    let stats = ledger::aggregate(&entries, since);

    if options.verbose {
        eprintln!(
            "xavier: ledger={} entries={} runs={}",
            path.display(),
            entries.len(),
            stats.runs
        );
    }

    if options.json {
        print!("{}", render_stats_json(&stats));
    } else {
        print!("{}", render_stats(&stats, &path));
    }
    0
}

fn record_run(path: &Path, entry: &LedgerEntry, options: &Options) {
    if let Err(error) = ledger::append(path, entry) {
        eprintln!("xavier: could not write ledger {}: {error}", path.display());
        return;
    }

    let Some(budget) = options.budget_usd else {
        return;
    };
    let spent = match ledger::read_entries(path) {
        Ok(entries) => {
            let since = ledger::window_start(Utc::now(), options.budget_window_seconds);
            ledger::window_spend(&entries, since)
        }
        Err(error) => {
            eprintln!(
                "xavier: could not re-read ledger {} for the budget check: {error}",
                path.display()
            );
            return;
        }
    };
    if round_usd(spent) > round_usd(budget) {
        eprintln!(
            "xavier: budget exceeded after this run: {} spent in the last {} (budget {}). Further runs will be refused.",
            format_usd(Some(spent)),
            format_duration_seconds(options.budget_window_seconds),
            format_usd(Some(budget))
        );
    }
}

fn run_replay(path: &Path, options: &Options) -> i32 {
    let source = match read_replay(path) {
        Ok(source) => source,
        Err(error) => {
            report(&error);
            return error.exit_code();
        }
    };

    let (raw, mut result) = match source {
        ReplaySource::Cache(entry) => {
            let entry = *entry;
            (entry.raw, entry.result)
        }
        ReplaySource::Raw(raw) => {
            let result = match build_result(&raw, options, 0) {
                Ok(result) => result,
                Err(error) => {
                    report(&error);
                    return error.exit_code();
                }
            };
            (raw, result)
        }
    };

    // Replay is offline and does not read or write the cache.
    result.meta.cache = None;

    if options.format == OutputFormat::Raw {
        println!(
            "{}",
            serde_json::to_string_pretty(&raw).expect("raw payload is serializable")
        );
        return 0;
    }

    print_result(&result, options.format);
    print_diagnostics(&result, options, Some(format!("replay={}", path.display())));

    0
}

fn print_result(result: &SearchResult, format: OutputFormat) {
    match format {
        OutputFormat::Json => print!("{}", render_json(result)),
        OutputFormat::Jsonl => print!("{}", render_jsonl(result)),
        OutputFormat::Md => print!("{}", render_markdown(result)),
        OutputFormat::Raw => unreachable!("raw is handled before rendering"),
    }
}

fn print_diagnostics(result: &SearchResult, options: &Options, cache: Option<String>) {
    if !options.verbose {
        return;
    }

    let mut diagnostics = Vec::new();
    if let Some(cache) = cache {
        diagnostics.push(cache);
    }
    diagnostics.push(format!(
        "model={} effort={} max_turns={}",
        options.model,
        options.effort.as_str(),
        options.max_turns
    ));
    for search in &result.meta.searches {
        let mut line = format!("search {}", search.name);
        if let Some(query) = &search.query {
            line.push_str(&format!(" \"{query}\""));
        }
        if let Some(mode) = &search.mode {
            line.push_str(&format!(" ({mode})"));
        }
        diagnostics.push(line);
    }
    diagnostics.push(format!(
        "response={} posts={} fetched={} cost={} elapsed={}ms",
        result.meta.response_id.as_deref().unwrap_or("unknown"),
        result.posts.len(),
        result.meta.usage.x_posts_fetched,
        format_usd(result.meta.usage.cost_usd),
        result.meta.elapsed_ms
    ));
    for line in diagnostics {
        eprintln!("xavier: {line}");
    }
}

fn report(error: &AppError) {
    eprintln!("xavier: {}", error.message());
    if let Some(hint) = error.hint() {
        eprintln!("  hint: {hint}");
    }
}

fn read_stdin() -> String {
    let mut bytes = Vec::new();
    let _ = io::stdin().read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).trim().to_string()
}
