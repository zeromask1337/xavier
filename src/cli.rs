use std::collections::HashMap;
use std::io::{self, IsTerminal, Read};

use chrono::Utc;

use crate::args::{ParsedCli, help_text, parse_cli};
use crate::capabilities;
use crate::cost::format_usd;
use crate::errors::{AppError, ErrorCode};
use crate::output::{render_json, render_jsonl, render_markdown};
use crate::xai::{build_request_body, build_result, request_x_posts};

pub fn run(argv: Vec<String>) -> i32 {
    let json_errors = json_errors_requested(&argv);
    let env: HashMap<String, String> = std::env::vars().collect();
    let now = Utc::now();

    let parsed = match parse_cli(&argv, &env, now) {
        Ok(parsed) => parsed,
        Err(error) => {
            if json_errors {
                report(&error, true);
            } else {
                eprint!("xavier: {}\n\n{}", error.message(), help_text());
            }
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
        ParsedCli::Capabilities => {
            print!("{}", capabilities::render_manifest());
            return 0;
        }
        ParsedCli::SchemaOutput => {
            print!("{}", capabilities::render_output_schema());
            return 0;
        }
        ParsedCli::Run(options) => options,
    };
    let json_errors = options.json_errors;

    let mut query = options.query.clone();
    if query == "-" || (query.is_empty() && !io::stdin().is_terminal()) {
        let piped = read_stdin();
        if query == "-" || !piped.is_empty() {
            query = piped;
        }
    }

    query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        let error = AppError::new(ErrorCode::Empty, "missing query.")
            .with_hint("Usage: xavier [search] <query> [options] or echo \"<query>\" | xavier -");
        report(&error, json_errors);
        return error.exit_code();
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

    let api_key = env.get("XAI_API_KEY").map(|value| value.trim().to_string());
    let Some(api_key) = api_key.filter(|value| !value.is_empty()) else {
        let error = AppError::missing_api_key();
        report(&error, json_errors);
        return error.exit_code();
    };

    let base_url = env.get("XAI_BASE_URL").map(String::as_str);
    let raw = match request_x_posts(&options, &api_key, base_url) {
        Ok(raw) => raw,
        Err(error) => {
            report(&error, json_errors);
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
        return 0;
    }

    let result = match build_result(&raw.raw, &options, raw.elapsed_ms) {
        Ok(result) => result,
        Err(error) => {
            report(&error, json_errors);
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
        for line in diagnostics {
            eprintln!("xavier: {line}");
        }
    }

    0
}

fn report(error: &AppError, json_errors: bool) {
    if json_errors {
        eprintln!("{}", error.to_json_string());
        return;
    }
    eprintln!("xavier: {}", error.message());
    if let Some(hint) = error.hint() {
        eprintln!("  hint: {hint}");
    }
}

fn json_errors_requested(argv: &[String]) -> bool {
    argv.iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| arg == "--json-errors" || arg.starts_with("--json-errors="))
}

fn read_stdin() -> String {
    let mut bytes = Vec::new();
    let _ = io::stdin().read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).trim().to_string()
}
