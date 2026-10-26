use std::collections::HashSet;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

use crate::args::{Options, to_exclusive_date};
use crate::cost::{CostInput, estimate_cost, round_usd};
use crate::dates::is_valid_date_string;
use crate::errors::{AppError, Result};
use crate::types::{CostSource, Meta, Post, SearchCall, SearchResult, Usage};

pub const API_BASE: &str = "https://api.x.ai/v1";
const TICKS_PER_USD: f64 = 10_000_000_000.0;

const SYSTEM_PROMPT: &str = r#"You are xavier, a search engine for X (Twitter) posts.

Use the x_search tool to find posts matching the user's request. Run several searches when that improves coverage, within the turn limit.

Return only JSON matching the schema:
- summary: 2-4 factual sentences synthesizing what the posts say. Note agreement and disagreement. No markdown, no URLs.
- posts: distinct, relevant posts, best first.

Hard rules:
- Only include posts you actually retrieved with x_search. Never invent a URL, handle, date, or quote.
- Use the exact post URL. Prefer https://x.com/<handle>/status/<id>.
- author: the handle without the leading @.
- date: the post's UTC date as YYYY-MM-DD, or null if unknown.
- excerpt: verbatim text from the post, trimmed to at most 280 characters. Do not paraphrase. Preserve the original language.
- Never include the same post twice, and never include a repost and its original as separate posts.
- If no posts match, return an empty posts array and say so in the summary.
- Respect the requested date range and authors; the tool parameters also enforce them."#;

pub struct RawResponse {
    pub raw: Value,
    pub elapsed_ms: u64,
}

pub fn build_request_body(options: &Options) -> Value {
    let mut tool = Map::new();
    tool.insert("type".to_string(), json!("x_search"));
    if !options.handles.is_empty() {
        tool.insert("allowed_x_handles".to_string(), json!(options.handles));
    }
    if !options.exclude_handles.is_empty() {
        tool.insert(
            "excluded_x_handles".to_string(),
            json!(options.exclude_handles),
        );
    }
    if let Some(from) = &options.from {
        tool.insert("from_date".to_string(), json!(from));
    }
    if let Some(to) = to_exclusive_date(options.to.as_deref()) {
        tool.insert("to_date".to_string(), json!(to));
    }
    if options.images {
        tool.insert("enable_image_understanding".to_string(), json!(true));
    }
    if options.videos {
        tool.insert("enable_video_understanding".to_string(), json!(true));
    }

    let mut body = json!({
        "model": options.model,
        "store": false,
        "input": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": build_user_prompt(options) },
        ],
        "tools": [Value::Object(tool)],
        "text": {
            "format": {
                "type": "json_schema",
                "name": "xavier_results",
                "strict": true,
                "schema": result_schema(options.limit),
            }
        },
        "max_turns": options.max_turns,
        "include": ["no_inline_citations"],
    });

    if supports_reasoning_effort(&options.model) {
        body["reasoning"] = json!({ "effort": options.effort.as_str() });
    }

    body
}

pub fn build_user_prompt(options: &Options) -> String {
    let mut lines = vec![
        format!("Search X for: {}", options.query),
        String::new(),
        "Constraints:".to_string(),
        format!("- Return at most {} posts.", options.limit),
    ];

    match (&options.from, &options.to) {
        (Some(from), Some(to)) => {
            lines.push(format!("- Posted from {from} to {to} inclusive (UTC)."))
        }
        (Some(from), None) => lines.push(format!("- Posted on or after {from} (UTC).")),
        (None, Some(to)) => lines.push(format!("- Posted on or before {to} (UTC).")),
        (None, None) => {}
    }

    if !options.handles.is_empty() {
        lines.push(format!(
            "- Only these authors: {}.",
            at_handles(&options.handles)
        ));
    }
    if !options.exclude_handles.is_empty() {
        lines.push(format!(
            "- Exclude these authors: {}.",
            at_handles(&options.exclude_handles)
        ));
    }
    if options.sort.is_some_and(|sort| sort.as_str() == "latest") {
        lines.push("- Prefer the newest posts; use Latest mode for keyword searches.".to_string());
    } else if options.sort.is_some_and(|sort| sort.as_str() == "top") {
        lines.push(
            "- Prefer highly engaged, relevant posts; use Top mode for keyword searches."
                .to_string(),
        );
    }

    lines.join("\n")
}

fn at_handles(handles: &[String]) -> String {
    handles
        .iter()
        .map(|handle| format!("@{handle}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn supports_reasoning_effort(model: &str) -> bool {
    if let Some(rest) = model.strip_prefix("grok-4.") {
        for version in ["5", "6", "7"] {
            if rest == version || rest.starts_with(&format!("{version}-")) {
                return true;
            }
        }
    }
    model.starts_with("grok-4.20")
}

fn result_schema(limit: usize) -> Value {
    json!({
        "type": "object",
        "properties": {
            "summary": {
                "type": "string",
                "description": "2-4 factual sentences synthesizing the matching posts.",
            },
            "posts": {
                "type": "array",
                "maxItems": limit,
                "items": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "format": "uri",
                            "description": "Exact X post status URL.",
                        },
                        "author": {
                            "type": "string",
                            "description": "Author handle without the leading @.",
                        },
                        "date": {
                            "type": ["string", "null"],
                            "description": "UTC date as YYYY-MM-DD, or null if unknown.",
                        },
                        "excerpt": {
                            "type": "string",
                            "description": "Verbatim excerpt of the post, at most 280 characters.",
                        },
                    },
                    "required": ["url", "author", "date", "excerpt"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["summary", "posts"],
        "additionalProperties": false,
    })
}

pub fn request_x_posts(
    options: &Options,
    api_key: &str,
    base_url_override: Option<&str>,
) -> Result<RawResponse> {
    let base_url = base_url_override
        .map(str::to_string)
        .or_else(|| std::env::var("XAI_BASE_URL").ok())
        .unwrap_or_else(|| API_BASE.to_string())
        .trim_end_matches('/')
        .to_string();
    let url = format!("{base_url}/responses");
    let body = serde_json::to_string(&build_request_body(options))
        .map_err(|error| AppError::runtime(format!("Could not serialize request: {error}")))?;

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(options.timeout_ms))
        .build();
    let started = Instant::now();
    let response = agent
        .post(&url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {api_key}"))
        .send_string(&body);

    let (status, text) = match response {
        Ok(response) => {
            let status = response.status();
            let text = response.into_string().map_err(|error| {
                AppError::runtime(format!("Could not read xAI response: {error}"))
            })?;
            (status, text)
        }
        Err(ureq::Error::Status(status, response)) => {
            let text = response.into_string().unwrap_or_default();
            (status, text)
        }
        Err(ureq::Error::Transport(transport)) => {
            let message = transport.to_string();
            let elapsed_ms = elapsed_millis(&started);
            let timed_out = elapsed_ms >= options.timeout_ms.saturating_sub(100)
                || message.to_ascii_lowercase().contains("timed out")
                || message.to_ascii_lowercase().contains("timeout");
            if timed_out {
                let seconds = (options.timeout_ms as f64 / 1000.0).round() as u64;
                return Err(AppError::runtime_with_hint(
                    format!("Request timed out after {seconds}s."),
                    "Raise --timeout or narrow the search.",
                ));
            }
            return Err(AppError::runtime_with_hint(
                format!("Could not reach xAI: {message}"),
                format!("Check network access to {base_url}."),
            ));
        }
    };

    let data: Value = match serde_json::from_str(&text) {
        Ok(data) => data,
        Err(_) => {
            if !(200..300).contains(&status) {
                return Err(api_error(status, None, &text));
            }
            let hint = truncate(text.trim(), 300);
            let error = AppError::runtime(format!("xAI returned non-JSON (HTTP {status})."));
            return Err(match hint {
                hint if !hint.is_empty() => AppError::runtime_with_hint(error.message(), hint),
                _ => error,
            });
        }
    };

    if !(200..300).contains(&status) {
        return Err(api_error(status, Some(&data), &text));
    }

    let elapsed_ms = elapsed_millis(&started);
    Ok(RawResponse {
        raw: data,
        elapsed_ms,
    })
}

pub fn build_result(data: &Value, options: &Options, elapsed_ms: u64) -> Result<SearchResult> {
    let empty = Map::new();
    let root = data.as_object().unwrap_or(&empty);

    if let Some(error) = root.get("error")
        && !error.is_null()
    {
        return Err(AppError::runtime(format!(
            "xAI API error: {}",
            error_message(Some(error)).unwrap_or_else(|| "null".to_string())
        )));
    }

    match as_string(root.get("status")) {
        Some("incomplete") => {
            let reason = root
                .get("incomplete_details")
                .and_then(Value::as_object)
                .and_then(|details| as_string(details.get("reason")))
                .unwrap_or("unknown reason");
            return Err(AppError::runtime_with_hint(
                format!("xAI response was incomplete: {reason}."),
                "Raise --max-turns, narrow the query, or retry.",
            ));
        }
        Some("failed") => {
            return Err(AppError::runtime_with_hint(
                "xAI response failed.",
                "Retry, or use --raw to inspect the payload.",
            ));
        }
        _ => {}
    }

    let Some(output_text) = extract_output_text(root) else {
        return Err(AppError::runtime_with_hint(
            "xAI returned no final message.",
            "Retry, raise --max-turns, or use --raw to inspect the payload.",
        ));
    };

    let Some(parsed_value) = parse_json_text(&output_text) else {
        return Err(AppError::runtime_with_hint(
            "Could not parse the structured result from xAI.",
            "Retry with a narrower query, or use --raw to inspect the payload.",
        ));
    };

    let Some(parsed) = parsed_value.as_object() else {
        return Err(AppError::runtime_with_hint(
            "Could not parse the structured result from xAI.",
            "Retry with a narrower query, or use --raw to inspect the payload.",
        ));
    };

    let summary = as_string(parsed.get("summary"))
        .unwrap_or("")
        .trim()
        .to_string();
    let mut warnings: Vec<String> = Vec::new();

    let mut posts = dedupe_posts(
        as_array(parsed.get("posts"))
            .iter()
            .filter_map(parse_post)
            .collect(),
    );

    let cited_ids = collect_citation_ids(root);
    if !posts.is_empty() {
        if cited_ids.is_empty() {
            warnings.push("No X citation URLs were returned; posts are unverified.".to_string());
        } else {
            let total = posts.len();
            posts.retain(|post| cited_ids.contains(&post.id));
            let dropped = total - posts.len();
            if dropped > 0 {
                warnings.push(format!(
                    "Dropped {dropped} post(s) that did not appear in X Search citations."
                ));
            }
            for post in &mut posts {
                post.verified = true;
            }
        }
    }

    if posts.len() > options.limit {
        posts.truncate(options.limit);
        warnings.push(format!("Truncated to {} posts.", options.limit));
    }

    let usage = extract_usage(root, &options.model, &mut warnings);
    let searches = collect_searches(root);

    let normalized_summary = summary.trim().to_ascii_lowercase();
    let is_stub = normalized_summary.is_empty()
        || matches!(
            normalized_summary.as_str(),
            "placeholder" | "todo" | "tbd" | "na" | "n/a"
        );
    if posts.is_empty() && is_stub && usage.x_posts_fetched > 0 {
        return Err(AppError::runtime_with_hint(
            format!(
                "xAI returned a placeholder result after fetching {} posts.",
                usage.x_posts_fetched
            ),
            "This usually means --max-turns was exhausted. Retry with a higher --max-turns or a narrower query.",
        ));
    }

    let response_id = as_string(root.get("id")).map(str::to_string);
    let model = as_string(root.get("model"))
        .unwrap_or(&options.model)
        .to_string();
    let created_at = unix_to_iso(as_number(root.get("created_at")))
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());

    let meta = Meta {
        model,
        response_id,
        created_at,
        elapsed_ms,
        effort: options.effort,
        max_turns: options.max_turns,
        filters: crate::types::Filters {
            from: options.from.clone(),
            to: options.to.clone(),
            handles: options.handles.clone(),
            exclude_handles: options.exclude_handles.clone(),
            limit: options.limit,
            sort: options.sort,
        },
        usage,
        searches,
        citations: cited_ids.len(),
        warnings,
    };

    Ok(SearchResult {
        query: options.query.clone(),
        summary,
        posts,
        meta,
    })
}

fn extract_output_text(root: &Map<String, Value>) -> Option<String> {
    let mut text: Option<String> = None;
    for item in as_array(root.get("output")) {
        let Some(message) = item.as_object() else {
            continue;
        };
        if as_string(message.get("type")) != Some("message") {
            continue;
        }
        for content in as_array(message.get("content")) {
            let Some(block) = content.as_object() else {
                continue;
            };
            if as_string(block.get("type")) != Some("output_text") {
                continue;
            }
            if let Some(candidate) = as_string(block.get("text"))
                && !candidate.trim().is_empty()
            {
                text = Some(candidate.to_string());
            }
        }
    }
    text
}

fn parse_json_text(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Some(value);
    }

    if let Some(fenced) = fenced_json(trimmed)
        && let Ok(value) = serde_json::from_str(fenced)
    {
        return Some(value);
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
        && end > start
        && let Ok(value) = serde_json::from_str(&trimmed[start..=end])
    {
        return Some(value);
    }

    None
}

fn fenced_json(text: &str) -> Option<&str> {
    let start = text.find("```")?;
    let mut after = &text[start + 3..];
    if after
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("json"))
    {
        after = &after[4..];
    }
    let after = after.trim_start();
    let end = after.find("```")?;
    Some(after[..end].trim())
}

struct CanonicalUrl {
    url: String,
    id: String,
    handle: Option<String>,
}

fn canonical_post_url(raw: &str) -> Option<CanonicalUrl> {
    let value = raw.trim();
    let without_scheme = strip_prefix_ignore_case(value, "https://")
        .or_else(|| strip_prefix_ignore_case(value, "http://"))
        .unwrap_or(value);
    let without_www = strip_prefix_ignore_case(without_scheme, "www.").unwrap_or(without_scheme);
    let rest = strip_prefix_ignore_case(without_www, "x.com/")
        .or_else(|| strip_prefix_ignore_case(without_www, "twitter.com/"))?;

    let mut segments = rest.split('/');
    let handle = segments.next()?;
    if !crate::args::is_valid_handle(handle) {
        return None;
    }

    let first = segments.next()?;
    let (status, id_segment) = if first.eq_ignore_ascii_case("web") {
        (segments.next()?, segments.next()?)
    } else {
        (first, segments.next()?)
    };

    if !(status.eq_ignore_ascii_case("status") || status.eq_ignore_ascii_case("statuses")) {
        return None;
    }

    let digit_count = id_segment.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 || digit_count > 25 {
        return None;
    }
    let id = &id_segment[..digit_count];
    let tail = &id_segment[digit_count..];
    if !tail.is_empty() && !matches!(tail.as_bytes()[0], b'/' | b'?' | b'#') {
        return None;
    }

    let is_bare = handle.eq_ignore_ascii_case("i");
    let url = if is_bare {
        format!("https://x.com/i/status/{id}")
    } else {
        format!("https://x.com/{handle}/status/{id}")
    };

    Some(CanonicalUrl {
        url,
        id: id.to_string(),
        handle: if is_bare {
            None
        } else {
            Some(handle.to_string())
        },
    })
}

fn parse_post(value: &Value) -> Option<Post> {
    let item = value.as_object()?;
    let raw_url = as_string(item.get("url"))?;
    let canonical = canonical_post_url(raw_url)?;

    let raw_author = as_string(item.get("author"))
        .unwrap_or("")
        .trim()
        .trim_start_matches('@');
    let author = if crate::args::is_valid_handle(raw_author) {
        raw_author.to_string()
    } else {
        canonical
            .handle
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    };

    let date = as_string(item.get("date"))
        .filter(|date| is_valid_date_string(date))
        .map(str::to_string);
    let excerpt = collapse_whitespace(as_string(item.get("excerpt")).unwrap_or(""));

    Some(Post {
        url: canonical.url,
        id: canonical.id,
        author,
        date,
        excerpt,
        verified: false,
    })
}

fn dedupe_posts(posts: Vec<Post>) -> Vec<Post> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for post in posts {
        if seen.insert(post.id.clone()) {
            output.push(post);
        }
    }
    output
}

fn collect_citation_ids(root: &Map<String, Value>) -> HashSet<String> {
    fn add_url(ids: &mut HashSet<String>, value: &Value) {
        if let Some(url) = value.as_str()
            && let Some(canonical) = canonical_post_url(url)
        {
            ids.insert(canonical.id);
        }
    }

    let mut ids = HashSet::new();

    for citation in as_array(root.get("citations")) {
        if citation.is_string() {
            add_url(&mut ids, citation);
        } else if let Some(url) = citation.as_object().and_then(|entry| entry.get("url")) {
            add_url(&mut ids, url);
        }
    }

    for item in as_array(root.get("output")) {
        let Some(record) = item.as_object() else {
            continue;
        };
        for annotation in as_array(record.get("annotations")) {
            let Some(entry) = annotation.as_object() else {
                continue;
            };
            if as_string(entry.get("type")) == Some("url_citation")
                && let Some(url) = entry.get("url")
            {
                add_url(&mut ids, url);
            }
        }
        for content in as_array(record.get("content")) {
            let Some(block) = content.as_object() else {
                continue;
            };
            for annotation in as_array(block.get("annotations")) {
                let Some(entry) = annotation.as_object() else {
                    continue;
                };
                if as_string(entry.get("type")) == Some("url_citation")
                    && let Some(url) = entry.get("url")
                {
                    add_url(&mut ids, url);
                }
            }
        }
    }

    ids
}

fn collect_searches(root: &Map<String, Value>) -> Vec<SearchCall> {
    let mut searches = Vec::new();
    for item in as_array(root.get("output")) {
        let Some(record) = item.as_object() else {
            continue;
        };
        if as_string(record.get("type")) != Some("custom_tool_call") {
            continue;
        }
        let Some(name) = as_string(record.get("name")) else {
            continue;
        };
        if !name.starts_with("x_") {
            continue;
        }

        let mut input = record.get("input").cloned().unwrap_or(Value::Null);
        if let Some(serialized) = input.as_str() {
            input = serde_json::from_str(serialized).unwrap_or(Value::Null);
        }
        let args = input.as_object();

        searches.push(SearchCall {
            name: name.to_string(),
            query: args
                .and_then(|args| as_string(args.get("query")))
                .map(str::to_string),
            mode: args
                .and_then(|args| as_string(args.get("mode")))
                .map(str::to_string),
        });
    }
    searches
}

fn extract_usage(root: &Map<String, Value>, model: &str, warnings: &mut Vec<String>) -> Usage {
    let usage = root.get("usage").and_then(Value::as_object);
    let input_details = usage
        .and_then(|usage| usage.get("input_tokens_details"))
        .and_then(Value::as_object);
    let output_details = usage
        .and_then(|usage| usage.get("output_tokens_details"))
        .and_then(Value::as_object);
    let tools = usage
        .and_then(|usage| usage.get("server_side_tool_usage_details"))
        .and_then(Value::as_object);

    let input = as_i64(usage.and_then(|usage| usage.get("input_tokens")));
    let cached = as_i64(input_details.and_then(|details| details.get("cached_tokens")));
    let output = as_i64(usage.and_then(|usage| usage.get("output_tokens")));
    let reasoning = as_i64(output_details.and_then(|details| details.get("reasoning_tokens")));
    let total = usage
        .and_then(|usage| usage.get("total_tokens"))
        .and_then(|value| as_number(Some(value)))
        .map(|value| value as i64)
        .unwrap_or(input + output);
    let x_calls = as_i64(tools.and_then(|tools| tools.get("x_search_calls")));
    let x_posts = as_i64(tools.and_then(|tools| tools.get("x_posts_fetched")));
    let x_users = as_i64(tools.and_then(|tools| tools.get("x_users_fetched")));

    let ticks = as_number(usage.and_then(|usage| usage.get("cost_in_usd_ticks")));
    let mut cost_usd = None;
    let mut cost_source = None;

    if let Some(ticks) = ticks {
        cost_usd = Some(round_usd(ticks / TICKS_PER_USD));
        cost_source = Some(CostSource::Api);
    } else if let Some(estimate) = estimate_cost(&CostInput {
        model,
        input,
        cached,
        output,
        x_posts,
        x_users,
    }) {
        cost_usd = Some(estimate);
        cost_source = Some(CostSource::Estimated);
        warnings.push("xAI did not return an exact cost; cost_usd is an estimate.".to_string());
    }

    Usage {
        input_tokens: input,
        cached_input_tokens: cached,
        output_tokens: output,
        reasoning_tokens: reasoning,
        total_tokens: total,
        x_search_calls: x_calls,
        x_posts_fetched: x_posts,
        x_users_fetched: x_users,
        cost_usd,
        cost_source,
    }
}

fn api_error(status: u16, data: Option<&Value>, text: &str) -> AppError {
    let message = data
        .and_then(Value::as_object)
        .and_then(|root| root.get("error"))
        .and_then(|error| error_message(Some(error)))
        .or_else(|| {
            if text.trim().is_empty() {
                None
            } else {
                Some(truncate(text, 500))
            }
        })
        .unwrap_or_else(|| format!("HTTP {status}"));

    let hint = match status {
        401 => Some("Check XAI_API_KEY; try `doppler run -- xavier ...`.".to_string()),
        403 => Some("This key may not have access to the model or the x_search tool.".to_string()),
        404 => Some("Model or endpoint not found. Check --model.".to_string()),
        429 => Some("Rate limited; wait and retry.".to_string()),
        status if status >= 500 => Some("xAI service error; retry shortly.".to_string()),
        _ => None,
    };

    let message = format!("xAI API error {status}: {message}");
    match hint {
        Some(hint) => AppError::runtime_with_hint(message, hint),
        None => AppError::runtime(message),
    }
}

fn error_message(error: Option<&Value>) -> Option<String> {
    let error = error?;
    if error.is_null() {
        return None;
    }
    if let Some(message) = error
        .as_object()
        .and_then(|object| as_string(object.get("message")))
    {
        return Some(message.to_string());
    }
    if let Some(message) = as_string(Some(error)) {
        return Some(message.to_string());
    }
    Some(truncate(
        &serde_json::to_string(error).unwrap_or_default(),
        500,
    ))
}

fn as_string(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

fn as_array(value: Option<&Value>) -> &[Value] {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn as_number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(number) = value.as_f64()
        && number.is_finite()
    {
        return Some(number);
    }
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let parsed = trimmed.parse::<f64>().ok()?;
            if parsed.is_finite() {
                return Some(parsed);
            }
        }
    }
    None
}

fn as_i64(value: Option<&Value>) -> i64 {
    as_number(value).unwrap_or(0.0) as i64
}

fn strip_prefix_ignore_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn elapsed_millis(started: &Instant) -> u64 {
    (started.elapsed().as_secs_f64() * 1000.0).round() as u64
}

fn unix_to_iso(seconds: Option<f64>) -> Option<String> {
    let seconds = seconds?;
    let millis = (seconds * 1000.0) as i64;
    DateTime::<Utc>::from_timestamp_millis(millis)
        .map(|date| date.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::OutputFormat;
    use crate::types::{Effort, SortMode};
    use serde_json::json;

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
        }
    }

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("fixture is valid JSON")
    }

    #[test]
    fn builds_a_bounded_non_stored_request() {
        let body = build_request_body(&Options {
            limit: 5,
            ..options()
        });
        assert_eq!(body["model"], "grok-4.7");
        assert_eq!(body["store"], false);
        assert_eq!(body["max_turns"], 2);
        assert_eq!(body["reasoning"], json!({ "effort": "low" }));
        assert_eq!(body["include"], json!(["no_inline_citations"]));
        assert_eq!(body["tools"], json!([{ "type": "x_search" }]));
        assert_eq!(body["text"]["format"]["type"], "json_schema");
        assert_eq!(body["text"]["format"]["strict"], true);
        assert_eq!(
            body["text"]["format"]["schema"]["properties"]["posts"]["maxItems"],
            5
        );
        assert_eq!(body["input"][0]["role"], "system");
        assert!(
            body["input"][1]["content"]
                .as_str()
                .is_some_and(|content| content.contains("Search X for: grok 4.7"))
        );
    }

    #[test]
    fn maps_filters_to_x_search_tool_parameters() {
        let body = build_request_body(&Options {
            handles: vec!["alice".to_string(), "bob".to_string()],
            from: Some("2026-10-18".to_string()),
            to: Some("2026-10-25".to_string()),
            images: true,
            videos: true,
            sort: Some(SortMode::Latest),
            ..options()
        });
        assert_eq!(
            body["tools"][0],
            json!({
                "type": "x_search",
                "allowed_x_handles": ["alice", "bob"],
                "from_date": "2026-10-18",
                "to_date": "2026-10-26",
                "enable_image_understanding": true,
                "enable_video_understanding": true,
            })
        );
        let prompt = body["input"][1]["content"].as_str().unwrap();
        assert!(prompt.contains("Only these authors: @alice, @bob"));
        assert!(prompt.contains("Latest mode"));
    }

    #[test]
    fn supports_excluded_handles_without_allow_lists() {
        let body = build_request_body(&Options {
            exclude_handles: vec!["carol".to_string()],
            ..options()
        });
        assert_eq!(body["tools"][0]["excluded_x_handles"], json!(["carol"]));
        assert!(body["tools"][0].get("allowed_x_handles").is_none());
    }

    #[test]
    fn omits_reasoning_for_models_without_effort_support() {
        let body = build_request_body(&Options {
            model: "grok-4.3".to_string(),
            ..options()
        });
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn states_limits_and_range_in_prompt() {
        let prompt = build_user_prompt(&Options {
            limit: 7,
            from: Some("2026-10-01".to_string()),
            to: Some("2026-10-25".to_string()),
            ..options()
        });
        assert!(prompt.contains("Return at most 7 posts."));
        assert!(prompt.contains("from 2026-10-01 to 2026-10-25 inclusive"));
    }

    #[test]
    fn validates_deduplicates_and_verifies_cited_posts() {
        let result = build_result(&fixture(), &options(), 1234).unwrap();

        assert_eq!(result.query, "grok 4.7");
        assert_eq!(result.summary, "Mixed reactions.");
        assert_eq!(result.posts.len(), 2);
        assert_eq!(
            result.posts[0],
            Post {
                url: "https://x.com/alice/status/111".to_string(),
                id: "111".to_string(),
                author: "alice".to_string(),
                date: Some("2026-10-24".to_string()),
                excerpt: "Grok 4.7 is great.".to_string(),
                verified: true,
            }
        );
        assert_eq!(result.posts[1].id, "222");
        assert_eq!(result.posts[1].date, None);

        assert_eq!(result.meta.model, "grok-4.7");
        assert_eq!(result.meta.response_id.as_deref(), Some("resp_test_123"));
        assert_eq!(result.meta.citations, 3);
        assert_eq!(result.meta.elapsed_ms, 1234);
        assert_eq!(
            result.meta.warnings,
            vec!["Dropped 1 post(s) that did not appear in X Search citations."]
        );
        assert_eq!(
            result.meta.searches,
            vec![
                SearchCall {
                    name: "x_keyword_search".to_string(),
                    query: Some("grok 4.7".to_string()),
                    mode: Some("Latest".to_string()),
                },
                SearchCall {
                    name: "x_semantic_search".to_string(),
                    query: Some("grok discussion".to_string()),
                    mode: None,
                },
            ]
        );
    }

    #[test]
    fn uses_the_exact_cost_returned_by_the_api() {
        let result = build_result(&fixture(), &options(), 0).unwrap();
        assert_eq!(result.meta.usage.cost_usd, Some(0.001234));
        assert_eq!(result.meta.usage.cost_source, Some(CostSource::Api));
        assert_eq!(result.meta.usage.x_posts_fetched, 3);
        assert_eq!(result.meta.usage.x_search_calls, 2);
        assert_eq!(result.meta.usage.cached_input_tokens, 100);
    }

    #[test]
    fn keeps_posts_but_flags_them_when_no_citations_are_returned() {
        let mut data = fixture();
        data["output"][2]["content"][0]
            .as_object_mut()
            .unwrap()
            .remove("annotations");

        let result = build_result(&data, &options(), 0).unwrap();
        assert_eq!(result.posts.len(), 3);
        assert!(result.posts.iter().all(|post| !post.verified));
        assert!(
            result
                .meta
                .warnings
                .contains(&"No X citation URLs were returned; posts are unverified.".to_string())
        );
    }

    #[test]
    fn drops_posts_that_do_not_match_returned_citations() {
        let mut data = fixture();
        data["output"][2]["content"][0]["annotations"] =
            json!([{ "type": "url_citation", "url": "https://x.com/i/status/999" }]);

        let result = build_result(&data, &options(), 0).unwrap();
        assert!(result.posts.is_empty());
        assert!(
            result.meta.warnings.contains(
                &"Dropped 3 post(s) that did not appear in X Search citations.".to_string()
            )
        );
    }

    #[test]
    fn enforces_the_limit() {
        let result = build_result(
            &fixture(),
            &Options {
                limit: 1,
                ..options()
            },
            0,
        )
        .unwrap();
        assert_eq!(result.posts.len(), 1);
        assert!(
            result
                .meta
                .warnings
                .contains(&"Truncated to 1 posts.".to_string())
        );
    }

    #[test]
    fn rejects_responses_without_a_final_message() {
        let error = build_result(&json!({ "output": [] }), &options(), 0).unwrap_err();
        assert!(error.message().contains("no final message"));
    }

    #[test]
    fn rejects_unparseable_structured_output() {
        let mut data = fixture();
        data["output"][2]["content"][0]["text"] = json!("not json");
        assert!(build_result(&data, &options(), 0).is_err());
    }

    #[test]
    fn rejects_placeholder_stubs_after_a_search() {
        let mut data = fixture();
        data["output"][2]["content"][0]["text"] = json!(r#"{"summary":"placeholder","posts":[]}"#);
        let error = build_result(&data, &options(), 0).unwrap_err();
        assert!(error.message().contains("placeholder result"));
    }

    #[test]
    fn rejects_incomplete_responses() {
        let mut data = fixture();
        data["status"] = json!("incomplete");
        data["incomplete_details"] = json!({ "reason": "max_turns" });
        data["output"] = json!([]);
        let error = build_result(&data, &options(), 0).unwrap_err();
        assert!(error.message().contains("incomplete"));
    }

    #[test]
    fn canonicalizes_post_urls() {
        let canonical = canonical_post_url("https://twitter.com/alice/status/123?s=20").unwrap();
        assert_eq!(canonical.url, "https://x.com/alice/status/123");
        assert_eq!(canonical.id, "123");

        let bare = canonical_post_url("x.com/i/web/status/999").unwrap();
        assert_eq!(bare.url, "https://x.com/i/status/999");
        assert_eq!(bare.handle, None);

        assert!(canonical_post_url("https://example.com/alice/status/1").is_none());
        assert!(canonical_post_url("https://x.com/alice/status/not-a-number").is_none());
    }
}
