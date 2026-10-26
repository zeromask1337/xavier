use serde::Serialize;

use crate::types::{CostSource, Post, SearchResult};

pub fn render_markdown(result: &SearchResult) -> String {
    let mut lines = vec![
        format!("# X search: {}", one_line(&result.query)),
        String::new(),
    ];

    let stats = format!(
        "model={} effort={} posts={} fetched={} searches={} cost_usd={} cost_source={} elapsed_ms={} limit={}",
        result.meta.model,
        result.meta.effort.as_str(),
        result.posts.len(),
        result.meta.usage.x_posts_fetched,
        result.meta.usage.x_search_calls,
        match result.meta.usage.cost_usd {
            Some(value) => format!("{value:.4}"),
            None => "unknown".to_string(),
        },
        match result.meta.usage.cost_source {
            Some(CostSource::Api) => "api",
            Some(CostSource::Estimated) => "estimated",
            None => "unknown",
        },
        result.meta.elapsed_ms,
        result.meta.filters.limit,
    );
    lines.push(stats);

    let mut filters = Vec::new();
    if let Some(from) = &result.meta.filters.from {
        filters.push(format!("from={from}"));
    }
    if let Some(to) = &result.meta.filters.to {
        filters.push(format!("to={to}"));
    }
    if !result.meta.filters.handles.is_empty() {
        filters.push(format!(
            "handles={}",
            at_handles(&result.meta.filters.handles)
        ));
    }
    if !result.meta.filters.exclude_handles.is_empty() {
        filters.push(format!(
            "exclude={}",
            at_handles(&result.meta.filters.exclude_handles)
        ));
    }
    if let Some(sort) = result.meta.filters.sort {
        filters.push(format!("sort={}", sort.as_str()));
    }
    if !filters.is_empty() {
        lines.push(filters.join(" "));
    }

    if !result.meta.warnings.is_empty() {
        lines.push(String::new());
        lines.push("## Warnings".to_string());
        for warning in &result.meta.warnings {
            lines.push(format!("- {warning}"));
        }
    }

    lines.push(String::new());
    lines.push("## Summary".to_string());
    lines.push(if result.summary.is_empty() {
        "No summary returned.".to_string()
    } else {
        result.summary.clone()
    });

    lines.push(String::new());
    lines.push("## Posts".to_string());

    if result.posts.is_empty() {
        lines.push("No matching posts.".to_string());
    } else {
        for (index, post) in result.posts.iter().enumerate() {
            let mut parts = vec![format!("{}. @{}", index + 1, post.author)];
            if let Some(date) = &post.date {
                parts.push(date.clone());
            }
            parts.push(post.url.clone());
            if !post.verified {
                parts.push("unverified".to_string());
            }
            lines.push(parts.join(" | "));
            if !post.excerpt.is_empty() {
                lines.push(format!("   > {}", post.excerpt));
            }
        }
    }

    format!("{}\n", lines.join("\n"))
}

pub fn render_json(result: &SearchResult) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(result).expect("SearchResult is serializable")
    )
}

pub fn render_jsonl(result: &SearchResult) -> String {
    #[derive(Serialize)]
    struct ResultLine<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        query: &'a str,
        summary: &'a str,
        meta: &'a crate::types::Meta,
    }

    #[derive(Serialize)]
    struct PostLine<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        url: &'a str,
        id: &'a str,
        author: &'a str,
        date: &'a Option<String>,
        excerpt: &'a str,
        verified: bool,
    }

    fn post_line(post: &Post) -> PostLine<'_> {
        PostLine {
            kind: "post",
            url: &post.url,
            id: &post.id,
            author: &post.author,
            date: &post.date,
            excerpt: &post.excerpt,
            verified: post.verified,
        }
    }

    let mut lines = vec![
        serde_json::to_string(&ResultLine {
            kind: "result",
            query: &result.query,
            summary: &result.summary,
            meta: &result.meta,
        })
        .expect("result line is serializable"),
    ];
    for post in &result.posts {
        lines.push(serde_json::to_string(&post_line(post)).expect("post line is serializable"));
    }

    format!("{}\n", lines.join("\n"))
}

fn at_handles(handles: &[String]) -> String {
    handles
        .iter()
        .map(|handle| format!("@{handle}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{Options, OutputFormat};
    use crate::types::{Effort, SortMode};
    use crate::xai::build_result;

    const FIXTURE: &str = include_str!("../tests/fixtures/response.json");

    fn options() -> Options {
        Options {
            query: "grok 4.7".to_string(),
            format: OutputFormat::Md,
            model: "grok-4.7".to_string(),
            handles: vec!["alice".to_string()],
            exclude_handles: Vec::new(),
            from: Some("2026-10-18".to_string()),
            to: Some("2026-10-25".to_string()),
            limit: 20,
            max_turns: 2,
            effort: Effort::Low,
            sort: Some(SortMode::Latest),
            images: false,
            videos: false,
            timeout_ms: 180_000,
            dry_run: false,
            verbose: false,
        }
    }

    fn result() -> SearchResult {
        let data: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
        build_result(&data, &options(), 9300).expect("fixture builds a result")
    }

    #[test]
    fn markdown_emits_stable_agent_readable_sections() {
        let markdown = render_markdown(&result());
        assert!(markdown.contains("# X search: grok 4.7"));
        assert!(
            markdown
                .contains("model=grok-4.7 effort=low posts=2 fetched=3 searches=2 cost_usd=0.0012")
        );
        assert!(markdown.contains("from=2026-10-18 to=2026-10-25 handles=@alice sort=latest"));
        assert!(markdown.contains("## Warnings"));
        assert!(markdown.contains("## Summary"));
        assert!(markdown.contains("## Posts"));
        assert!(markdown.contains("1. @alice | 2026-10-24 | https://x.com/alice/status/111"));
        assert!(markdown.contains("   > Grok 4.7 is great."));
    }

    #[test]
    fn markdown_renders_an_empty_result() {
        let mut empty = result();
        empty.posts.clear();
        empty.meta.warnings.clear();
        empty.summary.clear();
        let markdown = render_markdown(&empty);
        assert!(markdown.contains("No summary returned."));
        assert!(markdown.contains("No matching posts."));
    }

    #[test]
    fn markdown_marks_unverified_posts() {
        let mut unverified = result();
        for post in &mut unverified.posts {
            post.verified = false;
        }
        assert!(render_markdown(&unverified).contains("| unverified"));
    }

    #[test]
    fn json_round_trips_the_result_object() {
        let rendered = render_json(&result());
        let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed["query"], "grok 4.7");
        assert_eq!(parsed["posts"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["meta"]["usage"]["cost_usd"], 0.001234);
        assert_eq!(parsed["meta"]["usage"]["cost_source"], "api");
    }

    #[test]
    fn jsonl_emits_a_result_line_followed_by_post_lines() {
        let rendered = render_jsonl(&result());
        let lines: Vec<&str> = rendered.trim_end().lines().collect();
        assert_eq!(lines.len(), 3);

        let head: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(head["type"], "result");
        assert!(head.get("posts").is_none());
        assert_eq!(head["summary"], "Mixed reactions.");

        let post: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(post["type"], "post");
        assert_eq!(post["url"], "https://x.com/alice/status/111");
    }
}
