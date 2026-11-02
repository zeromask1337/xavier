use std::collections::HashMap;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::args::{LedgerMode, Options, ParsedCli, StatsOptions, help_text, parse_cli};
use crate::cost::{estimate_run_cost, format_usd, round_usd};
use crate::dates::format_duration_seconds;
use crate::errors::AppError;
use crate::ledger::{self, LedgerEntry, RunRecord};
use crate::output::{render_json, render_jsonl, render_markdown, render_stats, render_stats_json};
use crate::xai::{build_request_body, build_result, request_x_posts, summarize_raw};

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
        ParsedCli::Run(options) => options,
    };

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

    let base_url = env.get("XAI_BASE_URL").map(String::as_str);
    let raw = match request_x_posts(&options, &api_key, base_url) {
        Ok(raw) => raw,
        Err(error) => {
            report(&error);
            return error.exit_code();
        }
    };

    if options.format == crate::args::OutputFormat::Raw {
        println!(
            "{}",
            serde_json::to_string_pretty(&raw.raw).expect("raw payload is serializable")
        );
        if options.verbose {
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

    let result = match build_result(&raw.raw, &options, raw.elapsed_ms) {
        Ok(result) => result,
        Err(error) => {
            report(&error);
            return error.exit_code();
        }
    };

    match options.format {
        crate::args::OutputFormat::Json => print!("{}", render_json(&result)),
        crate::args::OutputFormat::Jsonl => print!("{}", render_jsonl(&result)),
        crate::args::OutputFormat::Md => print!("{}", render_markdown(&result)),
        crate::args::OutputFormat::Raw => unreachable!("raw is handled above"),
    }

    if options.verbose {
        let mut diagnostics = vec![format!(
            "model={} effort={} max_turns={}",
            options.model,
            options.effort.as_str(),
            options.max_turns
        )];
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
        if let Some(path) = &ledger_path {
            diagnostics.push(format!("ledger={}", path.display()));
        }
        for line in diagnostics {
            eprintln!("xavier: {line}");
        }
    }

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
