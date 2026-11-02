<img src="assets/icon.svg" width="64" height="64" alt="">

# xavier

[![crates.io](https://img.shields.io/crates/v/xavier-cli.svg)](https://crates.io/crates/xavier-cli)
[![docs.rs](https://img.shields.io/docsrs/xavier-cli)](https://docs.rs/xavier-cli)


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
`--json` and `--jsonl` are available for machine consumption. JSON and JSONL output
carry `schema_version: 1`, and `--json-errors` gives agents stable, retry-aware errors.

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
  "schema_version": 1,
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
    "warnings": [],
    "cache": { "hit": false, "age_seconds": null, "saved_usd": null }
  }
}
```

### Spend ledger and budget

Every successful run appends one line to an append-only JSONL ledger:

```json
{"ts":"2026-11-02T12:00:00.000Z","cost_usd":0.001234,"cost_source":"api","posts_fetched":3,"model":"grok-4.7","query_hash":"...","cached":false}
```

The default location is the platform data directory:

| Platform | Path |
| --- | --- |
| Linux | `$XDG_DATA_HOME/xavier/ledger.jsonl`, or `~/.local/share/xavier/ledger.jsonl` |
| macOS | `~/Library/Application Support/xavier/ledger.jsonl` |
| Windows | `%LOCALAPPDATA%\xavier\ledger.jsonl` |

`--ledger-dir <path>` and `XAVIER_LEDGER_DIR` override the directory. The flag wins
over the environment variable. On Unix the ledger file is created with `0600`
permissions.

Only a SHA-256 hash of the whitespace-normalized query is stored by default. Pass
`--ledger-query` if you need the query text in the ledger.

```sh
xavier stats                 # all-time totals
xavier stats --since 7d      # last 7 days
xavier stats --json          # machine-readable totals
```

`stats` reports run count, total cost, cache hits, fetched posts, and totals by day
and model. Corrupt or partial ledger lines are skipped. Cache hits are recorded with
`cost_usd = 0` and `cached = true`.

| Flag | Description |
| --- | --- |
| `--ledger <on\|off>` | Record successful runs. Default `on`. |
| `--ledger-dir <path>` | Ledger directory. |
| `--ledger-query` | Store query text. Off by default. |
| `--budget-usd <amount>` | Refuse to start when window spend plus the soft estimate would exceed this amount. |
| `--budget-window <duration>` | Budget window: `30d`, `12h`, `2w`. Default `30d`. |
| `--budget-multiplier <n>` | Preflight estimate multiplier. Default `3`. |

The preflight estimate is deliberately soft: `x_posts_fetched` can exceed `--limit`, so
it prices `--limit * --budget-multiplier` posts at $0.005 each. The check runs before
the request and refuses with exit code 4. After a response, the ledger is updated and
the hard check warns if the window now exceeds the budget; further runs are refused.
A budget cannot undo spend from the request that already completed.

### Cache

Successful results, including zero-post results, are cached on disk for 15 minutes by
default. Repeating the same request reuses the stored result instead of paying xAI again.
The key covers the query, base URL, model, effort, turn limit, post limit, date and author
filters, sort hint, and image/video flags. `meta.cache` reports whether a result was a hit,
how old the entry was, and the API cost the hit avoided; Markdown adds `cache=hit`.

| Flag | Description |
| --- | --- |
| `--cache <on\|off\|refresh>` | Read and write (`on`, default), bypass everything (`off`), or skip reads and replace the entry (`refresh`). |
| `--no-cache` | Alias for `--cache off`. |
| `--refresh` | Alias for `--cache refresh`. |
| `--cache-ttl <duration>` | Entry lifetime such as `15m`, `1h`, `7d`. Default 15 minutes. |
| `--cache-dir <path>` | Cache directory. `XAVIER_CACHE_DIR` also works. |
| `--replay <file>` | Render a raw xAI response or a xavier cache entry offline. |

The cache directory is `XAVIER_CACHE_DIR` when set, otherwise `~/Library/Caches/xavier`
on macOS, `%LOCALAPPDATA%\xavier` on Windows, or `$XDG_CACHE_HOME/xavier`
(`~/.cache/xavier` fallback) elsewhere. Entries are written atomically with `0600`
permissions. Cached post text is stored as plain JSON readable by your user account;
delete the cache directory to clear it. Corrupt or expired entries are ignored and removed.

Cache hits are resolved before `XAI_API_KEY` is checked, so repeat queries and `--replay`
work offline. `--raw` runs may read a cached payload but never write one; `--dry-run` does
not touch the cache. Replay does not require a key and never makes a request.

### Other flags

| Flag | Description |
| --- | --- |
| `--model <id>` | xAI model. Default `grok-4.7`. `XAVIER_MODEL` also works. |
| `--timeout <seconds>` | Request timeout, 5–600. Default 180. |
| `--json-errors` | On failure, write one JSON error object to stderr; stdout stays empty. |
| `-v, --verbose` | Search queries, response id, cost, and timing on stderr. |
| `-h, --help` | Usage. |
| `-V, --version` | Version. |

`XAI_BASE_URL` overrides the API base, including `/v1` (default
`https://api.x.ai/v1`).

### Introspection

```sh
xavier capabilities --json
xavier schema output
```

`capabilities` returns the name, version, `schema_version`, commands, flags, defaults,
limits, output formats, environment variables, exit codes, and every stable error code
with its retry policy. `schema output` returns a JSON Schema for the `--json`
`SearchResult` object.

### Errors

Failures use stable codes. With `--json-errors`, exactly one JSON object is written to
stderr and stdout stays empty:

```json
{"error":{"code":"rate_limited","message":"xAI API error 429: slow down","hint":"Rate limited; wait and retry.","retryable":true,"http_status":429}}
```

`hint` and `http_status` are omitted when they do not apply.

| Code | Exit | Retryable | Meaning |
| --- | --- | --- | --- |
| `usage` | 2 | no | Bad flag, conflicting option, or invalid value. |
| `missing_api_key` | 2 | no | `XAI_API_KEY` is missing or empty. |
| `auth` | 1 | no | The API rejected the key (HTTP 401). |
| `forbidden` | 1 | no | The key lacks access to the model or X Search (HTTP 403). |
| `not_found` | 1 | no | The model or endpoint does not exist (HTTP 404). |
| `rate_limited` | 1 | yes | The API rate limited the request (HTTP 429). |
| `server` | 1 | yes | The API failed internally (HTTP 5xx or a failed response). |
| `timeout` | 1 | yes | The request exceeded `--timeout`. |
| `network` | 1 | yes | xAI could not be reached. |
| `invalid_response` | 1 | no | The response was malformed or unparseable. |
| `incomplete` | 1 | yes | The model stopped before finishing; raise `--max-turns` or retry. |
| `placeholder` | 1 | yes | The model returned a placeholder instead of results. |
| `budget_exceeded` | 4 | no | The window spend plus the preflight estimate would exceed `--budget-usd`. |
| `empty` | 2 | no | No query was provided. |

Retry only when `retryable` is `true`, and back off between attempts. For `auth`,
`forbidden`, `usage`, and `missing_api_key`, fix the request or configuration instead.
Use `hint` for the suggested remediation.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success, including zero matches. |
| 1 | API, network, or response error. |
| 2 | Usage or configuration error (bad flag, missing key, empty query). |
| 4 | Budget refusal: the window spend plus the preflight estimate exceeds `--budget-usd`. |

## Architecture

- `src/cli.rs` — command flow: replay, cache lookup, budget preflight, request, store, ledger, render.
- `src/cache.rs` — SHA-256 cache keys, directory resolution, atomic load/store, replay parsing.
- `src/ledger.rs` — JSONL ledger, spend aggregation, stats, and budget checks.
- `src/xai.rs` — request construction, xAI HTTP, response normalization and citations.
- `src/output.rs` — Markdown, JSON, JSONL, and stats rendering.
- `src/args.rs` — flags, `CacheMode`, `LedgerMode`, `stats` parsing, option validation.
- `src/dates.rs` — date and duration parsing.
- `src/cost.rs`, `src/errors.rs`, `src/types.rs` — cost estimates, errors, shared types.
- `src/capabilities.rs` — stable error catalog, exit codes, and output schema for agents.
