use serde_json::{Value, json};

use crate::args::{
    DEFAULT_BUDGET_WINDOW_SECONDS, DEFAULT_CACHE_TTL_SECONDS, DEFAULT_LIMIT, DEFAULT_MAX_TURNS,
    DEFAULT_MODEL, DEFAULT_TIMEOUT_SECONDS,
};
use crate::cost::DEFAULT_BUDGET_MULTIPLIER;
use crate::errors::ErrorCode;
use crate::types::SCHEMA_VERSION;

/// Machine-readable command, flag, limit, and error-code manifest.
pub fn manifest() -> Value {
    let error_codes: Vec<Value> = ErrorCode::ALL
        .iter()
        .map(|code| {
            json!({
                "code": code.as_str(),
                "retryable": code.retryable(),
                "exit_code": code.exit_code(),
            })
        })
        .collect();

    json!({
        "name": "xavier",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Agent-first CLI for searching X posts via xAI X Search",
        "schema_version": SCHEMA_VERSION,
        "commands": [
            {
                "name": "search",
                "aliases": ["s"],
                "usage": "xavier [search] <query> [options]",
                "description": "Search X posts and print a summary plus cited posts."
            },
            {
                "name": "capabilities",
                "usage": "xavier capabilities [--json]",
                "description": "Print this manifest."
            },
            {
                "name": "schema",
                "usage": "xavier schema output",
                "description": "Print JSON schemas for machine-readable output.",
                "subcommands": [
                    {
                        "name": "output",
                        "description": "JSON Schema for the SearchResult object."
                    }
                ]
            },
            {
                "name": "stats",
                "usage": "xavier stats [--since 30d] [--json]",
                "description": "Summarize spend, cache hits, and fetched posts from the local ledger."
            },
            {
                "name": "help",
                "usage": "xavier --help",
                "description": "Print human-readable usage."
            },
            {
                "name": "version",
                "usage": "xavier --version",
                "description": "Print the CLI version."
            }
        ],
        "flags": [
            {
                "name": "--json-errors",
                "type": "boolean",
                "description": "Emit one JSON error object on stderr instead of prose."
            },
            {
                "name": "--json",
                "type": "boolean",
                "description": "Print one SearchResult JSON object."
            },
            {
                "name": "--jsonl",
                "type": "boolean",
                "description": "Print a result line followed by one line per post."
            },
            {
                "name": "--md",
                "type": "boolean",
                "description": "Print compact Markdown (default)."
            },
            {
                "name": "--raw",
                "type": "boolean",
                "description": "Print the raw xAI Responses API payload."
            },
            {
                "name": "--handle",
                "alias": "-u",
                "type": "string",
                "repeatable": true,
                "description": "Only these authors. Comma-separated values are split."
            },
            {
                "name": "--exclude-handle",
                "alias": "-x",
                "type": "string",
                "repeatable": true,
                "description": "Exclude these authors. Cannot be combined with --handle."
            },
            {
                "name": "--from",
                "alias": "--since",
                "type": "string",
                "description": "Start date: YYYY-MM-DD, 7d, 24h, today, yesterday."
            },
            {
                "name": "--to",
                "alias": "--until",
                "type": "string",
                "description": "Inclusive end date."
            },
            {
                "name": "--limit",
                "alias": "-n",
                "type": "integer",
                "description": "Maximum posts returned."
            },
            {
                "name": "--sort",
                "type": "string",
                "values": ["latest", "top"],
                "description": "Ranking hint."
            },
            {
                "name": "--max-turns",
                "type": "integer",
                "description": "Maximum agent turns."
            },
            {
                "name": "--effort",
                "type": "string",
                "values": ["low", "medium", "high", "xhigh"],
                "description": "Reasoning effort for Grok 4.5+ models."
            },
            {
                "name": "--images",
                "type": "boolean",
                "description": "Let the model view images inside posts."
            },
            {
                "name": "--videos",
                "type": "boolean",
                "description": "Let the model watch videos inside posts."
            },
            {
                "name": "--model",
                "type": "string",
                "description": "xAI model id."
            },
            {
                "name": "--timeout",
                "type": "integer",
                "description": "Request timeout in seconds."
            },
            {
                "name": "--dry-run",
                "type": "boolean",
                "description": "Print the request body and exit without calling xAI."
            },
            {
                "name": "--verbose",
                "alias": "-v",
                "type": "boolean",
                "description": "Diagnostics on stderr."
            },
            {
                "name": "--ledger",
                "type": "string",
                "values": ["on", "off"],
                "description": "Record successful runs in the local ledger."
            },
            {
                "name": "--ledger-dir",
                "type": "string",
                "description": "Ledger directory."
            },
            {
                "name": "--ledger-query",
                "type": "boolean",
                "description": "Store query text in the ledger instead of only its hash."
            },
            {
                "name": "--budget-usd",
                "type": "number",
                "description": "Refuse to run when the window spend plus the preflight estimate would exceed this amount."
            },
            {
                "name": "--budget-window",
                "type": "string",
                "description": "Budget window such as 30d, 12h, or 2w."
            },
            {
                "name": "--budget-multiplier",
                "type": "number",
                "description": "Soft preflight estimate multiplier."
            },
            {
                "name": "--cache",
                "type": "string",
                "values": ["on", "off", "refresh"],
                "description": "Read and write, bypass, or replace cached results."
            },
            {
                "name": "--no-cache",
                "type": "boolean",
                "description": "Alias for --cache off."
            },
            {
                "name": "--refresh",
                "type": "boolean",
                "description": "Alias for --cache refresh."
            },
            {
                "name": "--cache-ttl",
                "type": "string",
                "description": "Cache entry lifetime such as 15m, 1h, or 7d."
            },
            {
                "name": "--cache-dir",
                "type": "string",
                "description": "Cache directory."
            },
            {
                "name": "--replay",
                "type": "string",
                "description": "Render a raw response or cache entry offline."
            },
            {
                "name": "--help",
                "alias": "-h",
                "type": "boolean",
                "description": "Print usage."
            },
            {
                "name": "--version",
                "alias": "-V",
                "type": "boolean",
                "description": "Print the version."
            }
        ],
        "defaults": {
            "model": DEFAULT_MODEL,
            "limit": DEFAULT_LIMIT,
            "max_turns": DEFAULT_MAX_TURNS,
            "effort": "low",
            "timeout_seconds": DEFAULT_TIMEOUT_SECONDS,
            "output_format": "md",
            "ledger": "on",
            "budget_window_seconds": DEFAULT_BUDGET_WINDOW_SECONDS,
            "budget_multiplier": DEFAULT_BUDGET_MULTIPLIER,
            "cache": "on",
            "cache_ttl_seconds": DEFAULT_CACHE_TTL_SECONDS
        },
        "limits": {
            "limit": { "min": 1, "max": 100 },
            "max_turns": { "min": 1, "max": 10 },
            "timeout_seconds": { "min": 5, "max": 600 },
            "handles_per_list": 20,
            "query": { "min_length": 1 },
            "budget_usd": { "exclusive_minimum": 0 },
            "budget_multiplier": { "exclusive_minimum": 0 }
        },
        "output_formats": ["md", "json", "jsonl", "raw"],
        "environment_variables": [
            {
                "name": "XAI_API_KEY",
                "required": true,
                "description": "xAI API key."
            },
            {
                "name": "XAI_BASE_URL",
                "required": false,
                "default": "https://api.x.ai/v1",
                "description": "API base URL, including /v1."
            },
            {
                "name": "XAVIER_MODEL",
                "required": false,
                "default": DEFAULT_MODEL,
                "description": "Default model when --model is absent."
            },
            {
                "name": "XAVIER_LEDGER_DIR",
                "required": false,
                "description": "Ledger directory when --ledger-dir is absent."
            },
            {
                "name": "XAVIER_CACHE_DIR",
                "required": false,
                "description": "Cache directory when --cache-dir is absent."
            }
        ],
        "exit_codes": [
            { "code": 0, "meaning": "success" },
            { "code": 1, "meaning": "runtime error" },
            { "code": 2, "meaning": "usage or configuration error" },
            { "code": 4, "meaning": "budget exhausted or refused" }
        ],
        "error_codes": error_codes
    })
}

/// JSON Schema for the `SearchResult` object printed by `--json`.
pub fn output_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/zeromask1337/xavier/schema/output.json",
        "title": "SearchResult",
        "description": "A completed X search, as printed by xavier --json.",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "query", "summary", "posts", "meta"],
        "properties": {
            "schema_version": {
                "type": "integer",
                "const": SCHEMA_VERSION,
                "description": "Version of the machine-readable output contract."
            },
            "query": {
                "type": "string",
                "description": "Normalized query that was sent to xAI."
            },
            "summary": {
                "type": "string",
                "description": "Model-written synthesis of the matching posts."
            },
            "posts": {
                "type": "array",
                "items": { "$ref": "#/$defs/post" }
            },
            "meta": { "$ref": "#/$defs/meta" }
        },
        "$defs": {
            "post": {
                "type": "object",
                "additionalProperties": false,
                "required": ["url", "id", "author", "date", "excerpt", "verified"],
                "properties": {
                    "url": {
                        "type": "string",
                        "format": "uri",
                        "description": "Canonical x.com status URL."
                    },
                    "id": {
                        "type": "string",
                        "description": "Status id used for de-duplication."
                    },
                    "author": {
                        "type": "string",
                        "description": "Author handle without the leading @."
                    },
                    "date": {
                        "type": ["string", "null"],
                        "format": "date",
                        "description": "UTC date as YYYY-MM-DD, or null when unknown."
                    },
                    "excerpt": {
                        "type": "string",
                        "description": "Verbatim, whitespace-collapsed excerpt."
                    },
                    "verified": {
                        "type": "boolean",
                        "description": "True when the post URL appeared in X Search citations."
                    }
                }
            },
            "search_call": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "query", "mode"],
                "properties": {
                    "name": { "type": "string" },
                    "query": { "type": ["string", "null"] },
                    "mode": { "type": ["string", "null"] }
                }
            },
            "effort": {
                "type": "string",
                "enum": ["low", "medium", "high", "xhigh"]
            },
            "sort": {
                "type": "string",
                "enum": ["latest", "top"]
            },
            "cost_source": {
                "type": "string",
                "enum": ["api", "estimated"]
            },
            "filters": {
                "type": "object",
                "additionalProperties": false,
                "required": ["from", "to", "handles", "exclude_handles", "limit", "sort"],
                "properties": {
                    "from": { "type": ["string", "null"] },
                    "to": { "type": ["string", "null"] },
                    "handles": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "exclude_handles": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "limit": { "type": "integer", "minimum": 1 },
                    "sort": {
                        "anyOf": [
                            { "$ref": "#/$defs/sort" },
                            { "type": "null" }
                        ]
                    }
                }
            },
            "usage": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "input_tokens",
                    "cached_input_tokens",
                    "output_tokens",
                    "reasoning_tokens",
                    "total_tokens",
                    "x_search_calls",
                    "x_posts_fetched",
                    "x_users_fetched",
                    "cost_usd",
                    "cost_source"
                ],
                "properties": {
                    "input_tokens": { "type": "integer", "minimum": 0 },
                    "cached_input_tokens": { "type": "integer", "minimum": 0 },
                    "output_tokens": { "type": "integer", "minimum": 0 },
                    "reasoning_tokens": { "type": "integer", "minimum": 0 },
                    "total_tokens": { "type": "integer", "minimum": 0 },
                    "x_search_calls": { "type": "integer", "minimum": 0 },
                    "x_posts_fetched": { "type": "integer", "minimum": 0 },
                    "x_users_fetched": { "type": "integer", "minimum": 0 },
                    "cost_usd": { "type": ["number", "null"] },
                    "cost_source": {
                        "anyOf": [
                            { "$ref": "#/$defs/cost_source" },
                            { "type": "null" }
                        ]
                    }
                }
            },
            "meta": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "model",
                    "response_id",
                    "created_at",
                    "elapsed_ms",
                    "effort",
                    "max_turns",
                    "filters",
                    "usage",
                    "searches",
                    "citations",
                    "warnings"
                ],
                "properties": {
                    "model": { "type": "string" },
                    "response_id": { "type": ["string", "null"] },
                    "created_at": {
                        "type": "string",
                        "format": "date-time"
                    },
                    "elapsed_ms": { "type": "integer", "minimum": 0 },
                    "effort": { "$ref": "#/$defs/effort" },
                    "max_turns": { "type": "integer", "minimum": 1 },
                    "filters": { "$ref": "#/$defs/filters" },
                    "usage": { "$ref": "#/$defs/usage" },
                    "searches": {
                        "type": "array",
                        "items": { "$ref": "#/$defs/search_call" }
                    },
                    "citations": { "type": "integer", "minimum": 0 },
                    "warnings": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "cache": {
                        "anyOf": [
                            { "$ref": "#/$defs/cache_meta" },
                            { "type": "null" }
                        ]
                    }
                }
            },
            "cache_meta": {
                "type": "object",
                "additionalProperties": false,
                "required": ["hit", "age_seconds", "saved_usd"],
                "properties": {
                    "hit": { "type": "boolean" },
                    "age_seconds": { "type": ["integer", "null"], "minimum": 0 },
                    "saved_usd": { "type": ["number", "null"] }
                }
            }
        }
    })
}

pub fn render_manifest() -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest()).expect("manifest is serializable")
    )
}

pub fn render_output_schema() -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(&output_schema()).expect("schema is serializable")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_describes_the_cli_contract() {
        let manifest = manifest();
        assert_eq!(manifest["name"], "xavier");
        assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest["schema_version"], SCHEMA_VERSION);

        let commands = manifest["commands"].as_array().unwrap();
        for name in ["search", "capabilities", "schema", "stats"] {
            assert!(
                commands.iter().any(|command| command["name"] == name),
                "missing command {name}"
            );
        }

        let flags: Vec<&str> = manifest["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|flag| flag["name"].as_str().unwrap())
            .collect();
        for name in [
            "--json-errors",
            "--json",
            "--jsonl",
            "--limit",
            "--timeout",
            "--cache",
            "--ledger",
            "--budget-usd",
        ] {
            assert!(flags.contains(&name), "missing flag {name}");
        }

        assert_eq!(manifest["defaults"]["model"], DEFAULT_MODEL);
        assert_eq!(manifest["defaults"]["limit"], DEFAULT_LIMIT);
        assert_eq!(manifest["defaults"]["max_turns"], DEFAULT_MAX_TURNS);
        assert_eq!(manifest["limits"]["limit"], json!({ "min": 1, "max": 100 }));
        assert_eq!(
            manifest["output_formats"],
            json!(["md", "json", "jsonl", "raw"])
        );
        assert!(
            manifest["environment_variables"]
                .as_array()
                .unwrap()
                .iter()
                .any(|variable| variable["name"] == "XAI_API_KEY")
        );
    }

    #[test]
    fn manifest_lists_every_error_code() {
        let manifest = manifest();
        let codes = manifest["error_codes"].as_array().unwrap();
        assert_eq!(codes.len(), ErrorCode::ALL.len());
        for code in ErrorCode::ALL {
            assert!(
                codes.iter().any(|entry| entry["code"] == code.as_str()),
                "missing error code {}",
                code.as_str()
            );
        }
        let rate_limited = codes
            .iter()
            .find(|entry| entry["code"] == "rate_limited")
            .unwrap();
        assert_eq!(rate_limited["retryable"], true);
        assert_eq!(rate_limited["exit_code"], 1);
    }

    #[test]
    fn output_schema_has_resolvable_references() {
        let schema = output_schema();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["title"], "SearchResult");
        assert_eq!(schema["type"], "object");
        assert_eq!(
            schema["properties"]["schema_version"]["const"],
            SCHEMA_VERSION
        );
        assert_eq!(
            schema["properties"]["posts"]["items"]["$ref"],
            "#/$defs/post"
        );

        let definitions = schema["$defs"].as_object().unwrap();
        let mut refs = Vec::new();
        collect_refs(&schema, &mut refs);
        assert!(!refs.is_empty());
        for reference in refs {
            let name = reference
                .strip_prefix("#/$defs/")
                .unwrap_or_else(|| panic!("unexpected reference {reference}"));
            assert!(definitions.contains_key(name), "missing $def {name}");
        }
    }

    fn collect_refs(value: &Value, refs: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    if key == "$ref" {
                        refs.push(child.as_str().unwrap().to_string());
                    } else {
                        collect_refs(child, refs);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    collect_refs(item, refs);
                }
            }
            _ => {}
        }
    }
}
