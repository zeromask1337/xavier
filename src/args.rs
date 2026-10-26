use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::dates::{add_days, parse_when};
use crate::errors::{AppError, Result};
use crate::types::{Effort, SortMode};

pub const DEFAULT_MODEL: &str = "grok-4.7";
pub const DEFAULT_LIMIT: usize = 20;
pub const DEFAULT_MAX_TURNS: usize = 3;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Md,
    Json,
    Jsonl,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCli {
    Help,
    Version,
    Run(Options),
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
    match args.first().map(String::as_str) {
        Some("help") => return Ok(ParsedCli::Help),
        Some("version") => return Ok(ParsedCli::Version),
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

    Ok(ParsedCli::Run(Options {
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
    }))
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

OTHER
      --model <id>               xAI model (default {DEFAULT_MODEL})
      --timeout <seconds>        Request timeout (5-600, default {DEFAULT_TIMEOUT_SECONDS})
  -v, --verbose                  Diagnostics on stderr

AUTH
  XAI_API_KEY, for example:
    doppler run -- xavier "what are people saying about Grok 4.7"

EXAMPLES
  xavier "what are people saying about Grok 4.7"
  xavier search "rust async" --from 7d --limit 10 --json
  xavier "from:xai min_likes:100" --sort top
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
            ParsedCli::Run(options) => options,
            other => panic!("expected run, got {other:?}"),
        }
    }

    fn error(argv: &[&str]) -> AppError {
        let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_string()).collect();
        parse_cli(&argv, &env(&[]), now()).expect_err("expected a usage error")
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
    fn supports_attached_values() {
        let options = run(&["--limit=7", "--from=2026-10-01", "-n", "7"]);
        assert_eq!(options.limit, 7);
        assert_eq!(options.from.as_deref(), Some("2026-10-01"));
    }
}
