<img src="assets/icon.svg" width="64" height="64" alt="">

# xavier

[![crates.io](https://img.shields.io/crates/v/xavier-cli.svg)](https://crates.io/crates/xavier-cli)
[![docs.rs](https://img.shields.io/docsrs/xavier-cli)](https://docs.rs/xavier-cli)

Agent-first CLI for searching X posts, written in Rust.

```console
$ doppler run -- xavier "what are people saying about Grok 4.7" --limit 5 --from 7d

# X search: what are people saying about Grok 4.7

model=grok-4.7 effort=low posts=5 fetched=15 searches=3 cost_usd=0.1022 cost_source=api elapsed_ms=17712 limit=5
from=2026-10-18

## Summary
Reactions to Grok 4.7 since 18 October 2026 are mixed. Some users praise its honesty over
flattery and highlight real-time X reading, a 500k context window, and lower pricing than
rival models. Others call it useless compared with Grok 4.6, rushed, clipped, and
condescending, or not ready for enterprise production.

## Posts
1. @manojlds | 2026-10-24 | https://x.com/manojlds/status/2113987794490003550
   > I loved working with Grok 4.6. Grok 4.7 is useless. They better deliver something better than Sonnet 5.5
2. @DrKnowItAll16 | 2026-10-21 | https://x.com/DrKnowItAll16/status/2112880528668185069
   > No wonder 4o made me feel so good. 🤣 But yes--I love Grok 4.7 for its honesty, even when it hurts a bit.
...
```

Default output is compact Markdown with stable headings and a `key=value` metadata line.
`--json` and `--jsonl` are available for machine consumption.

## Why xAI X Search

X API v2 search needs an X developer app and an OAuth 2.0 bearer token. The available
credential here is `XAI_API_KEY`, so xavier uses xAI's [`x_search`](https://docs.x.ai/developers/tools/x-search)
tool. It searches X with keyword operators, semantic search, user search, and thread
fetches, then returns a synthesis plus cited post URLs.

If you need deterministic, exhaustive post objects with engagement metrics, add an
`X_BEARER_TOKEN` and an X API v2 backend. The two options compare like this:

| | xAI X Search (implemented) | X API v2 search |
| --- | --- | --- |
| Setup | `XAI_API_KEY` (already in Doppler) | X developer app + OAuth 2.0 bearer token + prepaid credits |
| Price | $5 / 1k posts fetched, $10 / 1k profiles, plus tokens | $0.005 / post read, deduplicated per 24h UTC day |
| Token cost | Grok 4.7: $2/M input, $0.50/M cached, $6/M output | none |
| Typical query | $0.06–$0.15 (measured) | $0.10 for 20 posts, $0.50 for 100 posts |
| Output | 2–4 sentence summary + cited posts | raw post objects: text, metrics, entities, media |
| Scope | keyword, semantic, user, thread; date + handle filters | recent search (7 days), or full archive (pay-per-use) |
| Determinism | model chooses queries; not exhaustive | exact query, pagination, repeatable |
| Latency | 9–20s | sub-second to a few seconds |
| Best for | agents exploring a topic | pipelines needing structured data at scale |

Both charge per post. xAI does not deduplicate across requests; the X API deduplicates
within a 24-hour UTC window. The X API caps pay-per-use at 3 million post reads per
monthly cycle.

## Install

Requires [Rust](https://rustup.rs) 1.88 or newer.

Install from [crates.io](https://crates.io/crates/xavier-cli):

```sh
cargo install xavier-cli
```

This installs the `xavier` binary. To build from a checkout instead:

```sh
cargo install --path .
```

Or build a standalone binary:

```sh
cargo build --release
./target/release/xavier --help
```

## Auth

`XAI_API_KEY` is read from the environment. With Doppler:

```sh
doppler run -- xavier "your query"
```

Or:

```sh
export XAI_API_KEY=xai-...
```

The key is never written to disk or included in output. Requests set `store: false` so
xAI does not retain the response for `previous_response_id`.

## Usage

```sh
xavier [search] <query> [options]
echo "<query>" | xavier -
```

Queries can be natural language or X search syntax:

```sh
xavier "what are people saying about Grok 4.7"
xavier 'from:xai "Grok 4.7" min_likes:50 -is:retweet' --sort top
xavier "rust async" --from 7d --limit 10 --json
xavier "AI agents" --handle karpathy,simonw --from 2026-10-01 --to 2026-10-25
```

### Filters

| Flag | Description |
| --- | --- |
| `-u, --handle <handle>` | Only these authors. Repeatable or comma-separated, max 20. |
| `-x, --exclude-handle <handle>` | Exclude these authors. Repeatable or comma-separated, max 20. Cannot combine with `--handle`. |
| `--from, --since <when>` | Start date. `YYYY-MM-DD`, `7d`, `24h`, `today`, `yesterday`. |
| `--to, --until <when>` | End date, inclusive. Same formats as `--from`. |
| `-n, --limit <n>` | Max posts returned, 1–100. Default 20. |
| `--sort <latest\|top>` | Ranking hint passed to the search. |
| `--max-turns <n>` | Max agent turns, 1–10. Default 3. Lower is cheaper. |
| `--effort <level>` | `low`, `medium`, `high`, `xhigh`. Default `low`. Grok 4.5+ only. |
| `--images` | Let the model view images inside matched posts. |
| `--videos` | Let the model watch videos inside matched posts. |

`--to` is inclusive: `--to 2026-10-25` covers all of 25 October UTC. Internally it is
converted to the exclusive `to_date` that xAI expects.

### Output

| Flag | Description |
| --- | --- |
| *(default)* | Compact Markdown with `## Summary`, `## Posts`, and a metadata line. |
| `--json` | One structured result object. |
| `--jsonl` | A `{"type":"result"}` line, then one `{"type":"post"}` line per post. |
| `--raw` | Raw xAI Responses API payload. |
| `--dry-run` | Print the request body and exit without spending credits. |

`--json` shape:

```json
{
  "query": "Grok 4.7 benchmark reactions",
  "summary": "Mixed reactions...",
  "posts": [
    {
      "url": "https://x.com/AdamHoltererer/status/2112518446965506124",
      "id": "2112518446965506124",
      "author": "AdamHoltererer",
      "date": "2026-10-20",
      "excerpt": "Grok 4.7 is a REGRESSION on almost half of shared benchmarks!",
      "verified": true
    }
  ],
  "meta": {
    "model": "grok-4.7",
    "response_id": "...",
    "created_at": "2026-10-25T20:10:00.000Z",
    "elapsed_ms": 17712,
    "effort": "low",
    "max_turns": 3,
    "filters": {
      "from": "2026-10-18",
      "to": null,
      "handles": [],
      "exclude_handles": [],
      "limit": 5,
      "sort": null
    },
    "usage": {
      "input_tokens": 9898,
      "cached_input_tokens": 2816,
      "output_tokens": 1657,
      "reasoning_tokens": 891,
      "total_tokens": 11555,
      "x_search_calls": 3,
      "x_posts_fetched": 15,
      "x_users_fetched": 0,
      "cost_usd": 0.100514,
      "cost_source": "api"
    },
    "searches": [
      { "name": "x_keyword_search", "query": "\"Grok 4.7\"", "mode": "Latest" }
    ],
    "citations": 15,
    "warnings": []
  }
}
```

### Other flags

| Flag | Description |
| --- | --- |
| `--model <id>` | xAI model. Default `grok-4.7`. `XAVIER_MODEL` also works. |
| `--timeout <seconds>` | Request timeout, 5–600. Default 180. |
| `-v, --verbose` | Search queries, response id, cost, and timing on stderr. |
| `-h, --help` | Usage. |
| `-V, --version` | Version. |

`XAI_BASE_URL` overrides the API base, including `/v1` (default
`https://api.x.ai/v1`).

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success, including zero matches. |
| 1 | API, network, or response error. |
| 2 | Usage or configuration error (bad flag, missing key). |

## Cost

The xAI response includes `usage.cost_in_usd_ticks`, which covers tokens and X Search
item fees. xavier prints that exact number as `cost_usd` with `cost_source=api`. If the
field is ever absent, xavier falls back to the published rates and marks
`cost_source=estimated`.

Measured with `grok-4.7` and `--effort low`:

| Run | Posts fetched | Cost |
| --- | --- | --- |
| 5 posts, 3 searches | 15 | $0.102 |
| 3 posts, 3 searches | 10 | $0.079 |
| 5 posts, 2 searches | 9 | $0.060 |

Cost scales with posts fetched and reasoning tokens, not with `--limit` alone. To spend
less: narrow `--from`/`--handle`, lower `--max-turns`, keep `--effort low`, and ask for
fewer posts. `--images` and `--videos` add image or video tokens to the bill.

## Verification

X Search server-side tool outputs are not returned to the client. xavier parses the
model's structured JSON and then checks every post URL against the citation annotations
on the response:

- Posts whose status id appears in the citations are marked `verified: true`.
- Posts that do not appear are dropped and counted in `meta.warnings`.
- If the API returns no X citations at all, posts are kept but marked `unverified` and a
  warning is emitted.
- A placeholder result after a real search (usually caused by exhausting `--max-turns`)
  is treated as an error instead of being returned as an empty result.

Excerpts are requested verbatim, but they are model output; treat them as excerpts, not
as a byte-exact mirror of the post. Fetch the post URL when exact text matters.

## Architecture

```
src/
  main.rs      process entry point
  cli.rs       stdin, auth, output selection, exit codes
  args.rs      flag parsing, validation, help text
  dates.rs     absolute and relative date parsing (UTC)
  xai.rs       Responses API request, x_search tool, result validation, cost
  output.rs    Markdown, JSON, and JSONL renderers
  cost.rs      exact-cost parsing and fallback estimates
  errors.rs    usage and runtime error types
  types.rs     result and metadata shapes
```

The backend boundary is `request_x_posts(&Options, &str, Option<&str>)` plus
`build_result(&Value, &Options, u64)`. Adding an X API v2 backend means implementing
that pair with `/2/tweets/search/recent` (or `/all`) and selecting it with a flag, without
changing the renderers or CLI surface.

## Development

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- --help
doppler run -- cargo run -- "Grok 4.7" --limit 5 --dry-run
cargo build --release
```

Tests cover argument parsing, date handling, request construction, citation validation,
cost math, all three output formats, and the CLI end to end against a local mock server.
No test touches the network or spends credits.

## Limitations

- Model-mediated search is not exhaustive. It may miss posts or pick different queries
  between runs.
- `--limit` is a maximum, not a guarantee.
- No engagement metrics, entities, or media metadata; use the X API for structured post
  objects.
- X Search counts every fetched post, and repeated fetches are not deduplicated.
- X Search does not document a hard 7-day window; date filters are passed to the tool.
- The summary contains model interpretation. Quote the linked posts when precision
  matters.
- Post text is untrusted input. URLs are checked against citations, but a hostile post can
  still influence the model's summary.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
