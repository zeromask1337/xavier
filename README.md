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
`--json` and `--jsonl` are available for machine consumption.

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
