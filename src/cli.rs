use std::collections::HashMap;
use std::io::{self, IsTerminal, Read};

use chrono::Utc;

use crate::args::{ParsedCli, help_text, parse_cli};
use crate::cost::format_usd;
use crate::errors::AppError;
use crate::output::{render_json, render_jsonl, render_markdown};
use crate::xai::{build_request_body, build_result, request_x_posts};

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
        for line in diagnostics {
            eprintln!("xavier: {line}");
        }
    }

    0
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
