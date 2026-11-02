use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::cost::DEFAULT_BUDGET_MULTIPLIER;
use crate::dates::{add_days, parse_duration_seconds, parse_when};
use crate::errors::{AppError, Result};
use crate::types::{Effort, SortMode};

pub const DEFAULT_MODEL: &str = "grok-4.7";
pub const DEFAULT_LIMIT: usize = 20;
pub const DEFAULT_MAX_TURNS: usize = 3;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 180;
pub const DEFAULT_BUDGET_WINDOW_SECONDS: u64 = 30 * 86_400;
pub const DEFAULT_CACHE_TTL_SECONDS: u64 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Md,
    Json,
    Jsonl,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerMode {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    On,
    Off,
    Refresh,
}

impl CacheMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Refresh => "refresh",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub query: String,
    pub format: OutputFormat,
    pub model: String,
    pub handles: Vec<String>,
    pub exclude_handles: Vec<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: usize,
    pub max_turns: usize,
    pub effort: Effort,
    pub sort: Option<SortMode>,
    pub images: bool,
    pub videos: bool,
    pub timeout_ms: u64,
    pub dry_run: bool,
    pub verbose: bool,
    pub json_errors: bool,
    pub ledger: LedgerMode,
    pub ledger_dir: Option<String>,
    pub ledger_query: bool,
    pub budget_usd: Option<f64>,
    pub budget_window_seconds: u64,
    pub budget_multiplier: f64,
    pub cache: CacheMode,
    pub cache_ttl_seconds: u64,
    pub cache_dir: Option<PathBuf>,
    pub replay: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsOptions {
    pub json: bool,
    pub since_seconds: Option<u64>,
    pub ledger_dir: Option<String>,
    pub verbose: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCli {
    Help,
    Version,
    Capabilities,
    SchemaOutput,
    Run(Box<Options>),
    Stats(StatsOptions),
}

#[derive(Debug, Default)]
struct RawValues {
    help: bool,
    version: bool,
    json: bool,
    jsonl: bool,
    md: bool,
    raw: bool,
    model: Option<String>,
    handle: Vec<String>,
    exclude_handle: Vec<String>,
    from: Option<String>,
    since: Option<String>,
    to: Option<String>,
    until: Option<String>,
    limit: Option<String>,
    max_turns: Option<String>,
    effort: Option<String>,
    sort: Option<String>,
    images: bool,
    videos: bool,
    timeout: Option<String>,
    dry_run: bool,
    verbose: bool,
    json_errors: bool,
    ledger: Option<String>,
    ledger_dir: Option<String>,
    ledger_query: bool,
    budget_usd: Option<String>,
    budget_window: Option<String>,
    budget_multiplier: Option<String>,
    cache: Option<String>,
    no_cache: bool,
    refresh: bool,
    cache_ttl: Option<String>,
    cache_dir: Option<String>,
    replay: Option<String>,
}

pub fn parse_cli(
    argv: &[String],
    env: &HashMap<String, String>,
    now: DateTime<Utc>,
) -> Result<ParsedCli> {
    let mut values = RawValues::default();
    let mut positionals = Vec::new();
    let mut index = 0;
    let mut positional_only = false;

    while index < argv.len() {
        let arg = &argv[index];

        if positional_only {
            positionals.push(arg.clone());
            index += 1;
            continue;
        }

        if arg == "--" {
            positional_only = true;
            index += 1;
            continue;
        }

        if let Some(long) = arg.strip_prefix("--") {
            let (name, attached) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            let mut consumed = 1;

            match name {
                "help" => set_bool(&mut values.help, attached, "--help")?,
                "version" => set_bool(&mut values.version, attached, "--version")?,
                "json" => set_bool(&mut values.json, attached, "--json")?,
                "jsonl" => set_bool(&mut values.jsonl, attached, "--jsonl")?,
                "md" => set_bool(&mut values.md, attached, "--md")?,
                "raw" => set_bool(&mut values.raw, attached, "--raw")?,
                "images" => set_bool(&mut values.images, attached, "--images")?,
                "videos" => set_bool(&mut values.videos, attached, "--videos")?,
                "dry-run" => set_bool(&mut values.dry_run, attached, "--dry-run")?,
                "verbose" => set_bool(&mut values.verbose, attached, "--verbose")?,
                "json-errors" => set_bool(&mut values.json_errors, attached, "--json-errors")?,
                "ledger-query" => set_bool(&mut values.ledger_query, attached, "--ledger-query")?,
                "ledger" => {
                    values.ledger = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--ledger",
                    )?)
                }
                "ledger-dir" => {
                    values.ledger_dir = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--ledger-dir",
                    )?)
                }
                "budget-usd" => {
                    values.budget_usd = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--budget-usd",
                    )?)
                }
                "budget-window" => {
                    values.budget_window = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--budget-window",
                    )?)
                }
                "budget-multiplier" => {
                    values.budget_multiplier = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--budget-multiplier",
                    )?)
                }
                "no-cache" => set_bool(&mut values.no_cache, attached, "--no-cache")?,
                "refresh" => set_bool(&mut values.refresh, attached, "--refresh")?,
                "cache" => {
                    values.cache =
                        Some(take_value(attached, argv, index, &mut consumed, "--cache")?)
                }
                "cache-ttl" => {
                    values.cache_ttl = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--cache-ttl",
                    )?)
                }
                "cache-dir" => {
                    values.cache_dir = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--cache-dir",
                    )?)
                }
                "replay" => {
                    values.replay = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--replay",
                    )?)
                }
                "model" => {
                    values.model =
                        Some(take_value(attached, argv, index, &mut consumed, "--model")?)
                }
                "handle" => values.handle.push(take_value(
                    attached,
                    argv,
                    index,
                    &mut consumed,
                    "--handle",
                )?),
                "exclude-handle" => values.exclude_handle.push(take_value(
                    attached,
                    argv,
                    index,
                    &mut consumed,
                    "--exclude-handle",
                )?),
                "from" => {
                    values.from = Some(take_value(attached, argv, index, &mut consumed, "--from")?)
                }
                "since" => {
                    values.since =
                        Some(take_value(attached, argv, index, &mut consumed, "--since")?)
                }
                "to" => values.to = Some(take_value(attached, argv, index, &mut consumed, "--to")?),
                "until" => {
                    values.until =
                        Some(take_value(attached, argv, index, &mut consumed, "--until")?)
                }
                "limit" => {
                    values.limit =
                        Some(take_value(attached, argv, index, &mut consumed, "--limit")?)
                }
                "max-turns" => {
                    values.max_turns = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--max-turns",
                    )?)
                }
                "effort" => {
                    values.effort = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--effort",
                    )?)
                }
                "sort" => {
                    values.sort = Some(take_value(attached, argv, index, &mut consumed, "--sort")?)
                }
                "timeout" => {
                    values.timeout = Some(take_value(
                        attached,
                        argv,
                        index,
                        &mut consumed,
                        "--timeout",
                    )?)
                }
                _ => return Err(AppError::usage(format!("Unknown option '--{name}'."))),
            }

            index += consumed;
            continue;
        }

        if arg.len() > 1 && arg.starts_with('-') {
            let chars: Vec<char> = arg[1..].chars().collect();
            let mut position = 0;
            let mut consumed = 1;

            while position < chars.len() {
                let flag = chars[position];
                match flag {
                    'h' => values.help = true,
                    'V' => values.version = true,
                    'v' => values.verbose = true,
                    'u' | 'x' | 'n' => {
                        let rest: String = chars[position + 1..].iter().collect();
                        let long_name = match flag {
                            'u' => "--handle",
                            'x' => "--exclude-handle",
                            _ => "--limit",
                        };
                        let value = if rest.is_empty() {
                            take_value(None, argv, index, &mut consumed, long_name)?
                        } else {
                            // Like util.parseArgs, the rest of the cluster is the value.
                            rest.strip_prefix('=').unwrap_or(&rest).to_string()
                        };
                        match flag {
                            'u' => values.handle.push(value),
                            'x' => values.exclude_handle.push(value),
                            _ => values.limit = Some(value),
                        }
                        position = chars.len();
                        continue;
                    }
                    other => {
                        return Err(AppError::usage(format!("Unknown option '-{other}'.")));
                    }
                }
                position += 1;
            }

            index += consumed;
            continue;
        }

        positionals.push(arg.clone());
        index += 1;
    }

    if values.help {
        return Ok(ParsedCli::Help);
    }
    if values.version {
        return Ok(ParsedCli::Version);
    }

    let mut args = positionals;
    if args.first().map(String::as_str) == Some("stats") {
        args.remove(0);
        return parse_stats(args, &values);
    }
    match args.first().map(String::as_str) {
        Some("help") => return Ok(ParsedCli::Help),
        Some("version") => return Ok(ParsedCli::Version),
        Some("capabilities") => {
            if args.len() > 1 {
                return Err(AppError::usage(
                    "The capabilities command takes no arguments.",
                ));
            }
            return Ok(ParsedCli::Capabilities);
        }
        Some("schema") => {
            return match args.get(1).map(String::as_str) {
                Some("output") if args.len() == 2 => Ok(ParsedCli::SchemaOutput),
                _ => Err(AppError::usage("Usage: xavier schema output")),
            };
        }
        Some("search" | "s") => {
            args.remove(0);
        }
        _ => {}
    }

    let query = args.join(" ").trim().to_string();

    let mut selected = Vec::new();
    if values.json {
        selected.push(("json", OutputFormat::Json));
    }
    if values.jsonl {
        selected.push(("jsonl", OutputFormat::Jsonl));
    }
    if values.md {
        selected.push(("md", OutputFormat::Md));
    }
    if values.raw {
        selected.push(("raw", OutputFormat::Raw));
    }
    if selected.len() > 1 {
        let flags: Vec<String> = selected
            .iter()
            .map(|(name, _)| format!("--{name}"))
            .collect();
        return Err(AppError::usage(format!(
            "Choose one output format, not {}.",
            flags.join(" and ")
        )));
    }
    let format = selected
        .first()
        .map(|(_, format)| *format)
        .unwrap_or(OutputFormat::Md);

    let cache = parse_cache_mode(&values)?;
    let cache_ttl_seconds = match values.cache_ttl.as_deref() {
        Some(raw) => parse_duration_seconds(raw)?,
        None => DEFAULT_CACHE_TTL_SECONDS,
    };
    let cache_dir = match values.cache_dir {
        Some(raw) => Some(parse_path_value(raw, "--cache-dir")?),
        None => None,
    };
    let replay = match values.replay {
        Some(raw) => Some(parse_path_value(raw, "--replay")?),
        None => None,
    };
    if replay.is_some() && values.dry_run {
        return Err(AppError::usage(
            "Use either --replay or --dry-run, not both.",
        ));
    }

    let handles = collect_handles(&values.handle, "--handle")?;
    let exclude_handles = collect_handles(&values.exclude_handle, "--exclude-handle")?;
    if !handles.is_empty() && !exclude_handles.is_empty() {
        return Err(AppError::usage(
            "Use either --handle or --exclude-handle, not both.",
        ));
    }

    let from_raw = pick_one(
        values.from.as_deref(),
        values.since.as_deref(),
        "--from",
        "--since",
    )?;
    let to_raw = pick_one(
        values.to.as_deref(),
        values.until.as_deref(),
        "--to",
        "--until",
    )?;
    let from = match from_raw {
        Some(value) => Some(parse_when(value, &now)?),
        None => None,
    };
    let to = match to_raw {
        Some(value) => Some(parse_when(value, &now)?),
        None => None,
    };
    if let (Some(from), Some(to)) = (&from, &to)
        && from > to
    {
        return Err(AppError::usage(format!(
            "--from ({from}) must not be after --to ({to})."
        )));
    }

    let limit = int_option(values.limit.as_deref(), "--limit", DEFAULT_LIMIT, 1, 100)?;
    let max_turns = int_option(
        values.max_turns.as_deref(),
        "--max-turns",
        DEFAULT_MAX_TURNS,
        1,
        10,
    )?;
    let timeout_seconds = int_option(
        values.timeout.as_deref(),
        "--timeout",
        DEFAULT_TIMEOUT_SECONDS as usize,
        5,
        600,
    )?;

    let model = values
        .model
        .or_else(|| env.get("XAVIER_MODEL").cloned())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
        .trim()
        .to_string();
    if model.is_empty() || model.chars().any(char::is_whitespace) {
        return Err(AppError::usage(format!(
            "Invalid --model value \"{model}\"."
        )));
    }

    let effort = parse_effort(values.effort.as_deref())?;
    let sort = parse_sort(values.sort.as_deref())?;
    let ledger = parse_ledger_mode(values.ledger.as_deref())?;
    let ledger_dir = parse_directory(values.ledger_dir.as_deref(), "--ledger-dir")?;
    let budget_usd = parse_money(values.budget_usd.as_deref(), "--budget-usd")?;
    let budget_window_seconds = match values.budget_window.as_deref() {
        Some(raw) => parse_duration_seconds(raw)?,
        None => DEFAULT_BUDGET_WINDOW_SECONDS,
    };
    let budget_multiplier = match values.budget_multiplier.as_deref() {
        Some(raw) => parse_multiplier(raw)?,
        None => DEFAULT_BUDGET_MULTIPLIER,
    };
    if budget_usd.is_none() {
        if values.budget_window.is_some() {
            return Err(AppError::usage("--budget-window requires --budget-usd."));
        }
        if values.budget_multiplier.is_some() {
            return Err(AppError::usage(
                "--budget-multiplier requires --budget-usd.",
            ));
        }
    } else if ledger == LedgerMode::Off {
        return Err(AppError::usage("--budget-usd requires --ledger on."));
    }

    Ok(ParsedCli::Run(Box::new(Options {
        query,
        format,
        model,
        handles,
        exclude_handles,
        from,
        to,
        limit,
        max_turns,
        effort,
        sort,
        images: values.images,
        videos: values.videos,
        timeout_ms: timeout_seconds as u64 * 1000,
        dry_run: values.dry_run,
        verbose: values.verbose,
        json_errors: values.json_errors,
        ledger,
        ledger_dir,
        ledger_query: values.ledger_query,
        budget_usd,
        budget_window_seconds,
        budget_multiplier,
        cache,
        cache_ttl_seconds,
        cache_dir,
        replay,
    })))
}

fn parse_cache_mode(values: &RawValues) -> Result<CacheMode> {
    let selected = usize::from(values.cache.is_some())
        + usize::from(values.no_cache)
        + usize::from(values.refresh);
    if selected > 1 {
        return Err(AppError::usage(
            "Use only one of --cache, --no-cache, and --refresh.",
        ));
    }
    if values.no_cache {
        return Ok(CacheMode::Off);
    }
    if values.refresh {
        return Ok(CacheMode::Refresh);
    }
    match values.cache.as_deref() {
        None => Ok(CacheMode::On),
        Some(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "on" => Ok(CacheMode::On),
            "off" => Ok(CacheMode::Off),
            "refresh" => Ok(CacheMode::Refresh),
            _ => Err(AppError::usage("--cache must be on, off, or refresh.")),
        },
    }
}

fn parse_path_value(raw: String, flag: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage(format!("{flag} requires a file path.")));
    }
    Ok(PathBuf::from(trimmed))
}

fn parse_stats(positionals: Vec<String>, values: &RawValues) -> Result<ParsedCli> {
    if let Some(extra) = positionals.first() {
        return Err(AppError::usage(format!(
            "stats does not accept the argument \"{extra}\"."
        )));
    }
    if values.jsonl || values.md || values.raw {
        return Err(AppError::usage("stats supports only --json."));
    }
    let rejected = [
        (values.model.is_some(), "--model"),
        (!values.handle.is_empty(), "--handle"),
        (!values.exclude_handle.is_empty(), "--exclude-handle"),
        (values.from.is_some(), "--from"),
        (values.to.is_some(), "--to"),
        (values.until.is_some(), "--until"),
        (values.limit.is_some(), "--limit"),
        (values.max_turns.is_some(), "--max-turns"),
        (values.effort.is_some(), "--effort"),
        (values.sort.is_some(), "--sort"),
        (values.timeout.is_some(), "--timeout"),
        (values.images, "--images"),
        (values.videos, "--videos"),
        (values.dry_run, "--dry-run"),
        (values.ledger_query, "--ledger-query"),
        (values.budget_usd.is_some(), "--budget-usd"),
        (values.budget_window.is_some(), "--budget-window"),
        (values.budget_multiplier.is_some(), "--budget-multiplier"),
        (values.cache.is_some(), "--cache"),
        (values.no_cache, "--no-cache"),
        (values.refresh, "--refresh"),
        (values.cache_ttl.is_some(), "--cache-ttl"),
        (values.cache_dir.is_some(), "--cache-dir"),
        (values.replay.is_some(), "--replay"),
    ];
    if let Some((_, flag)) = rejected.iter().find(|(set, _)| *set) {
        return Err(AppError::usage(format!("stats does not accept {flag}.")));
    }
    if parse_ledger_mode(values.ledger.as_deref())? == LedgerMode::Off {
        return Err(AppError::usage(
            "stats requires the ledger; remove --ledger off.",
        ));
    }
    let since_seconds = match values.since.as_deref() {
        Some(raw) => Some(parse_duration_seconds(raw)?),
        None => None,
    };
    Ok(ParsedCli::Stats(StatsOptions {
        json: values.json,
        since_seconds,
        ledger_dir: parse_directory(values.ledger_dir.as_deref(), "--ledger-dir")?,
        verbose: values.verbose,
    }))
}

fn parse_ledger_mode(raw: Option<&str>) -> Result<LedgerMode> {
    let Some(raw) = raw else {
        return Ok(LedgerMode::On);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "on" => Ok(LedgerMode::On),
        "off" => Ok(LedgerMode::Off),
        _ => Err(AppError::usage("--ledger must be on or off.")),
    }
}

fn parse_directory(raw: Option<&str>, flag: &str) -> Result<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage(format!("{flag} must not be empty.")));
    }
    Ok(Some(trimmed.to_string()))
}

fn parse_money(raw: Option<&str>, flag: &str) -> Result<Option<f64>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim().trim_start_matches('$');
    let value = trimmed
        .parse::<f64>()
        .map_err(|_| AppError::usage(format!("{flag} must be a positive dollar amount.")))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(AppError::usage(format!(
            "{flag} must be a positive dollar amount."
        )));
    }
    Ok(Some(value))
}

fn parse_multiplier(raw: &str) -> Result<f64> {
    let value = raw
        .trim()
        .parse::<f64>()
        .map_err(|_| AppError::usage("--budget-multiplier must be a positive number."))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(AppError::usage(
            "--budget-multiplier must be a positive number.",
        ));
    }
    Ok(value)
}

/// Inclusive calendar end date => exclusive xAI `to_date`.
pub fn to_exclusive_date(to: Option<&str>) -> Option<String> {
    to.and_then(|date| add_days(date, 1).ok())
}

fn set_bool(slot: &mut bool, attached: Option<&str>, flag: &str) -> Result<()> {
    if attached.is_some() {
        return Err(AppError::usage(format!(
            "Option '{flag}' does not take an argument."
        )));
    }
    *slot = true;
    Ok(())
}

fn take_value(
    attached: Option<&str>,
    argv: &[String],
    index: usize,
    consumed: &mut usize,
    flag: &str,
) -> Result<String> {
    if let Some(value) = attached {
        return Ok(value.to_string());
    }
    let next = index + 1;
    if next >= argv.len() {
        return Err(AppError::usage(format!(
            "Option '{flag} <value>' argument missing."
        )));
    }
    *consumed = 2;
    Ok(argv[next].clone())
}

fn collect_handles(raw: &[String], flag: &str) -> Result<Vec<String>> {
    let mut handles: Vec<String> = Vec::new();
    for chunk in raw {
        for part in chunk.split(',') {
            let handle = part.trim().trim_start_matches('@');
            if handle.is_empty() {
                continue;
            }
            if !is_valid_handle(handle) {
                return Err(AppError::usage(format!(
                    "{flag}: \"{}\" is not a valid X handle.",
                    part.trim()
                )));
            }
            if !handles
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(handle))
            {
                handles.push(handle.to_string());
            }
        }
    }
    if handles.len() > 20 {
        return Err(AppError::usage(format!(
            "{flag}: at most 20 handles are allowed."
        )));
    }
    Ok(handles)
}

pub(crate) fn is_valid_handle(value: &str) -> bool {
    (1..=15).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn pick_one<'a>(
    first: Option<&'a str>,
    second: Option<&'a str>,
    first_name: &str,
    second_name: &str,
) -> Result<Option<&'a str>> {
    if first.is_some() && second.is_some() {
        return Err(AppError::usage(format!(
            "Use only one of {first_name} and {second_name}."
        )));
    }
    Ok(first.or(second))
}

fn int_option(
    raw: Option<&str>,
    flag: &str,
    fallback: usize,
    min: usize,
    max: usize,
) -> Result<usize> {
    let Some(raw) = raw else {
        return Ok(fallback);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::usage(format!(
            "{flag} must be a whole number between {min} and {max}."
        )));
    }
    let value = trimmed.parse::<usize>().ok();
    match value {
        Some(value) if value >= min && value <= max => Ok(value),
        _ => Err(AppError::usage(format!(
            "{flag} must be between {min} and {max}."
        ))),
    }
}

fn parse_effort(raw: Option<&str>) -> Result<Effort> {
    let Some(raw) = raw else {
        return Ok(Effort::Low);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "low" => Ok(Effort::Low),
        "medium" => Ok(Effort::Medium),
        "high" => Ok(Effort::High),
        "xhigh" => Ok(Effort::Xhigh),
        _ => Err(AppError::usage(
            "--effort must be low, medium, high, or xhigh.",
        )),
    }
}

fn parse_sort(raw: Option<&str>) -> Result<Option<SortMode>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "latest" | "recent" => Ok(Some(SortMode::Latest)),
        "top" | "relevance" | "relevant" => Ok(Some(SortMode::Top)),
        _ => Err(AppError::usage("--sort must be latest or top.")),
    }
}

pub fn help_text() -> String {
    format!(
        r#"xavier — search X posts from the terminal

USAGE
  xavier [search] <query> [options]
  echo "<query>" | xavier -
  xavier capabilities [--json]
  xavier schema output
  xavier stats [--since 30d] [--json]
  xavier --help
  xavier --version

QUERY
  Natural language, or X search syntax such as:
    from:xai "Grok 4.7" min_likes:50 -is:retweet
  The backend turns it into one or more X searches.

OUTPUT
  Default      Compact Markdown (stable headings + key=value metadata)
  --json       One structured JSON result object
  --jsonl      Result metadata line, then one post per line
  --raw        Raw xAI Responses API payload
  --dry-run    Print the API request body and exit without spending credits
  --replay <file>  Render a raw response or cache entry offline

CACHE
      --cache <mode>             on | off | refresh (default on)
      --no-cache                 Alias for --cache off
      --refresh                  Alias for --cache refresh
      --cache-ttl <duration>     Entry lifetime: 15m, 1h, 7d (default 15m)
      --cache-dir <path>         Cache directory (or XAVIER_CACHE_DIR)

FILTERS
  -u, --handle <handle>          Only these authors (repeatable, comma-separated, max 20)
  -x, --exclude-handle <handle>  Exclude these authors (repeatable, comma-separated, max 20)
      --from, --since <when>     Start date: YYYY-MM-DD, 7d, 24h, today
      --to, --until <when>       End date, inclusive: YYYY-MM-DD, today
  -n, --limit <n>                Max posts to return (1-100, default {DEFAULT_LIMIT})
      --sort <latest|top>        Ranking hint
      --max-turns <n>            Max agent turns (1-10, default {DEFAULT_MAX_TURNS})
      --effort <level>           low | medium | high | xhigh (Grok 4.5+, default low)
      --images                   Let the model view images inside posts
      --videos                   Let the model watch videos inside posts

LEDGER
      --ledger <on|off>          Record successful runs (default on)
      --ledger-dir <path>        Ledger directory (also XAVIER_LEDGER_DIR)
      --ledger-query             Store query text; default stores only a hash
      --budget-usd <amount>      Refuse to run when window spend would exceed it (exit 4)
      --budget-window <duration> Budget window: 30d, 12h, 2w (default 30d)
      --budget-multiplier <n>    Soft preflight estimate multiplier (default 3)

OTHER
      --model <id>               xAI model (default {DEFAULT_MODEL})
      --timeout <seconds>        Request timeout (5-600, default {DEFAULT_TIMEOUT_SECONDS})
      --json-errors              One JSON error object on stderr
  -v, --verbose                  Diagnostics on stderr

INTROSPECTION
  xavier capabilities [--json]   Commands, flags, defaults, limits, formats, env
  xavier schema output           JSON Schema for the SearchResult object

AUTH
  XAI_API_KEY, for example:
    doppler run -- xavier "what are people saying about Grok 4.7"

EXAMPLES
  xavier "what are people saying about Grok 4.7"
  xavier search "rust async" --from 7d --limit 10 --json
  xavier "from:xai min_likes:100" --sort top
  xavier "grok 4.7" --budget-usd 5 --budget-window 30d
  xavier stats --since 7d
  xavier - < query.txt
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()
    }

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    fn run(argv: &[&str]) -> Options {
        options_with(argv, &[])
    }

    fn options_with(argv: &[&str], env_pairs: &[(&str, &str)]) -> Options {
        let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_string()).collect();
        match parse_cli(&argv, &env(env_pairs), now()).expect("expected a run command") {
            ParsedCli::Run(options) => *options,
            other => panic!("expected run, got {other:?}"),
        }
    }

    fn error(argv: &[&str]) -> AppError {
        let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_string()).collect();
        parse_cli(&argv, &env(&[]), now()).expect_err("expected a usage error")
    }

    fn stats(argv: &[&str]) -> StatsOptions {
        let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_string()).collect();
        match parse_cli(&argv, &env(&[]), now()).expect("expected a stats command") {
            ParsedCli::Stats(options) => options,
            other => panic!("expected stats, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_bare_query_with_defaults() {
        let options = run(&["what are people saying about Grok 4.7"]);
        assert_eq!(options.query, "what are people saying about Grok 4.7");
        assert_eq!(options.format, OutputFormat::Md);
        assert_eq!(options.model, "grok-4.7");
        assert_eq!(options.limit, 20);
        assert_eq!(options.max_turns, 3);
        assert_eq!(options.effort, Effort::Low);
        assert_eq!(options.timeout_ms, 180_000);
        assert_eq!(options.cache, CacheMode::On);
        assert_eq!(options.cache_ttl_seconds, 900);
        assert_eq!(options.cache_dir, None);
        assert_eq!(options.replay, None);
    }

    #[test]
    fn supports_the_search_subcommand_and_joined_positionals() {
        assert_eq!(run(&["search", "grok", "4.7"]).query, "grok 4.7");
        assert_eq!(run(&["s", "grok"]).query, "grok");
    }

    #[test]
    fn returns_help_and_version() {
        let args = |value: &str| vec![value.to_string()];
        assert_eq!(
            parse_cli(&args("--help"), &env(&[]), now()).unwrap(),
            ParsedCli::Help
        );
        assert_eq!(
            parse_cli(&args("help"), &env(&[]), now()).unwrap(),
            ParsedCli::Help
        );
        assert_eq!(
            parse_cli(&args("--version"), &env(&[]), now()).unwrap(),
            ParsedCli::Version
        );
        assert_eq!(
            parse_cli(&args("version"), &env(&[]), now()).unwrap(),
            ParsedCli::Version
        );
    }

    #[test]
    fn parses_introspection_commands() {
        let parse = |argv: &[&str]| {
            let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_string()).collect();
            parse_cli(&argv, &env(&[]), now())
        };

        assert_eq!(parse(&["capabilities"]).unwrap(), ParsedCli::Capabilities);
        assert_eq!(
            parse(&["capabilities", "--json"]).unwrap(),
            ParsedCli::Capabilities
        );
        assert_eq!(
            parse(&["schema", "output"]).unwrap(),
            ParsedCli::SchemaOutput
        );
        assert!(parse(&["schema", "output", "--json"]).is_ok());

        assert!(error(&["capabilities", "extra"]).is_usage());
        assert!(error(&["schema"]).is_usage());
        assert!(error(&["schema", "input"]).is_usage());
        assert!(error(&["schema", "output", "extra"]).is_usage());
    }

    #[test]
    fn captures_the_json_errors_flag() {
        assert!(run(&["grok", "--json-errors"]).json_errors);
        assert!(!run(&["grok"]).json_errors);
    }

    #[test]
    fn normalizes_and_deduplicates_handles() {
        let options = run(&["-u", "@alice,bob", "--handle", "ALICE"]);
        assert_eq!(options.handles, vec!["alice", "bob"]);
        assert_eq!(run(&["-x", "carol"]).exclude_handles, vec!["carol"]);
    }

    #[test]
    fn rejects_too_many_handles_and_mixed_allow_deny() {
        let many = (0..21)
            .map(|index| format!("user{index}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(error(&["--handle", &many]).is_usage());
        assert!(error(&["--handle", "alice", "--exclude-handle", "bob"]).is_usage());
    }

    #[test]
    fn parses_date_filters_and_validates_order() {
        let options = run(&["--from", "7d", "--to", "today"]);
        assert_eq!(options.from.as_deref(), Some("2026-10-18"));
        assert_eq!(options.to.as_deref(), Some("2026-10-25"));
        assert_eq!(
            to_exclusive_date(options.to.as_deref()).as_deref(),
            Some("2026-10-26")
        );
        assert!(error(&["--from", "2026-10-25", "--to", "2026-10-18"]).is_usage());
        assert!(error(&["--from", "1d", "--since", "2d"]).is_usage());
    }

    #[test]
    fn enforces_numeric_ranges() {
        assert_eq!(run(&["--limit", "5"]).limit, 5);
        assert!(error(&["--limit", "0"]).is_usage());
        assert!(error(&["--limit", "101"]).is_usage());
        assert!(error(&["--limit", "five"]).is_usage());
        assert!(error(&["--max-turns", "11"]).is_usage());
        assert!(error(&["--timeout", "4"]).is_usage());
    }

    #[test]
    fn parses_sort_and_effort_aliases() {
        assert_eq!(run(&["--sort", "recent"]).sort, Some(SortMode::Latest));
        assert_eq!(run(&["--sort", "relevance"]).sort, Some(SortMode::Top));
        assert_eq!(run(&["--effort", "xhigh"]).effort, Effort::Xhigh);
        assert!(error(&["--sort", "random"]).is_usage());
        assert!(error(&["--effort", "max"]).is_usage());
    }

    #[test]
    fn rejects_conflicting_output_formats() {
        assert!(error(&["--json", "--raw"]).is_usage());
        assert_eq!(run(&["--json"]).format, OutputFormat::Json);
        assert_eq!(run(&["--jsonl"]).format, OutputFormat::Jsonl);
        assert_eq!(run(&["--raw"]).format, OutputFormat::Raw);
    }

    #[test]
    fn reads_model_from_env_and_rejects_bad_models() {
        assert_eq!(
            options_with(&[], &[("XAVIER_MODEL", "grok-4.6")]).model,
            "grok-4.6"
        );
        assert!(error(&["--model", "grok 4"]).is_usage());
    }

    #[test]
    fn rejects_unknown_flags() {
        assert!(error(&["--nope"]).is_usage());
        assert!(error(&["-z"]).is_usage());
    }

    #[test]
    fn captures_media_and_dry_run_flags() {
        let options = run(&["--images", "--videos", "--dry-run"]);
        assert!(options.images);
        assert!(options.videos);
        assert!(options.dry_run);
    }

    #[test]
    fn parses_ledger_and_budget_flags() {
        let defaults = run(&["grok"]);
        assert_eq!(defaults.ledger, LedgerMode::On);
        assert_eq!(defaults.ledger_dir, None);
        assert!(!defaults.ledger_query);
        assert_eq!(defaults.budget_usd, None);
        assert_eq!(
            defaults.budget_window_seconds,
            DEFAULT_BUDGET_WINDOW_SECONDS
        );
        assert_eq!(defaults.budget_multiplier, DEFAULT_BUDGET_MULTIPLIER);

        let options = run(&[
            "grok",
            "--ledger",
            "off",
            "--ledger-dir",
            "/tmp/xavier",
            "--ledger-query",
        ]);
        assert_eq!(options.ledger, LedgerMode::Off);
        assert_eq!(options.ledger_dir.as_deref(), Some("/tmp/xavier"));
        assert!(options.ledger_query);

        let budget = run(&[
            "grok",
            "--budget-usd",
            "$0.50",
            "--budget-window",
            "12h",
            "--budget-multiplier",
            "2",
        ]);
        assert_eq!(budget.budget_usd, Some(0.5));
        assert_eq!(budget.budget_window_seconds, 43_200);
        assert_eq!(budget.budget_multiplier, 2.0);

        assert!(error(&["--ledger", "maybe"]).is_usage());
        assert!(error(&["--ledger-dir", ""]).is_usage());
        assert!(error(&["--budget-usd", "0"]).is_usage());
        assert!(error(&["--budget-usd", "-1"]).is_usage());
        assert!(error(&["--budget-usd", "free"]).is_usage());
        assert!(error(&["--budget-window", "30"]).is_usage());
        assert!(error(&["--budget-multiplier", "0"]).is_usage());
        assert!(error(&["--budget-window", "7d"]).is_usage());
        assert!(error(&["--budget-multiplier", "2"]).is_usage());
        assert!(error(&["--budget-usd", "5", "--ledger", "off"]).is_usage());
    }

    #[test]
    fn parses_the_stats_subcommand() {
        let options = stats(&["stats", "--since", "7d", "--json", "--verbose"]);
        assert!(options.json);
        assert_eq!(options.since_seconds, Some(7 * 86_400));
        assert!(options.verbose);

        let all = stats(&["stats"]);
        assert!(!all.json);
        assert_eq!(all.since_seconds, None);
        assert_eq!(all.ledger_dir, None);

        let with_dir = stats(&["stats", "--ledger-dir", "/tmp/ledger"]);
        assert_eq!(with_dir.ledger_dir.as_deref(), Some("/tmp/ledger"));

        assert!(error(&["stats", "--since", "last week"]).is_usage());
        assert!(error(&["stats", "--limit", "5"]).is_usage());
        assert!(error(&["stats", "--jsonl"]).is_usage());
        assert!(error(&["stats", "--ledger", "off"]).is_usage());
        assert!(error(&["stats", "extra"]).is_usage());
        assert!(error(&["stats", "--cache", "off"]).is_usage());
    }

    #[test]
    fn parses_cache_modes_and_aliases() {
        assert_eq!(run(&["--cache", "off"]).cache, CacheMode::Off);
        assert_eq!(run(&["--cache=refresh"]).cache, CacheMode::Refresh);
        assert_eq!(run(&["--cache", "on"]).cache, CacheMode::On);
        assert_eq!(run(&["--no-cache"]).cache, CacheMode::Off);
        assert_eq!(run(&["--refresh"]).cache, CacheMode::Refresh);
        assert!(error(&["--cache", "maybe"]).is_usage());
        assert!(error(&["--no-cache", "--refresh"]).is_usage());
        assert!(error(&["--cache", "off", "--no-cache"]).is_usage());
    }

    #[test]
    fn parses_cache_ttl_and_paths() {
        assert_eq!(run(&["--cache-ttl", "1h"]).cache_ttl_seconds, 3_600);
        assert_eq!(run(&["--cache-ttl=7d"]).cache_ttl_seconds, 604_800);
        assert_eq!(
            run(&["--cache-dir", "/tmp/xavier-cache"]).cache_dir,
            Some(PathBuf::from("/tmp/xavier-cache"))
        );
        assert_eq!(
            run(&["--replay", "result.json"]).replay,
            Some(PathBuf::from("result.json"))
        );
        assert!(error(&["--cache-ttl", "15"]).is_usage());
        assert!(error(&["--cache-dir", ""]).is_usage());
        assert!(error(&["--replay", ""]).is_usage());
        assert!(error(&["--replay", "result.json", "--dry-run"]).is_usage());
    }

    #[test]
    fn supports_attached_values() {
        let options = run(&["--limit=7", "--from=2026-10-01", "-n", "7"]);
        assert_eq!(options.limit, 7);
        assert_eq!(options.from.as_deref(), Some("2026-10-01"));
    }
}
