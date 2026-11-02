# Spend ledger, budget guard, and stats

## Goal

Make X Search spend visible and bounded. A typical run costs $0.06-$0.15; agent loops
and repeated queries add up with no local record.

## Scope

- Append-only JSONL ledger of successful runs:
  `{ts, cost_usd, cost_source, posts_fetched, model, query_hash, cached}`.
- Default location: XDG data directory (`~/.local/share/xavier/ledger.jsonl` on Linux,
  `~/Library/Application Support/xavier/` on macOS, `%LOCALAPPDATA%\xavier` on Windows).
- `xavier stats [--since 30d] [--json]`: run count, total cost, cache hits, totals by
  day and model.
- `--budget-usd <amount>` and `--budget-window <duration>` (default `30d`): refuse to
  start when ledger spend plus a soft estimate exceeds the budget. Exit code 4.
- `--ledger <on|off>`, `--ledger-dir <path>`, `XAVIER_LEDGER_DIR`.
- Privacy: store only a query hash by default; `--ledger-query` opts into storing text.
- Corrupt ledger lines are skipped; appends are small and atomic on supported platforms.

## Budget semantics

`x_posts_fetched` can exceed `--limit` (measured: 5 returned, 15 fetched), so preflight
cost is a soft estimate. Use a configurable multiplier, then enforce the hard check
after the response and before the next run. A budget cannot undo spend from the current
request.

## Implementation sketch

- New `src/ledger.rs`: path resolution, append, parse, aggregate, budget check.
- `src/args.rs`: flags plus `stats` subcommand parsing.
- `src/cli.rs`: load ledger before the request, record after a successful result,
  dispatch `stats`.
- `src/types.rs`: optional `meta.ledger` with cumulative window spend? Prefer no output
  schema change beyond `stats`; keep this PR additive.
- README: ledger location, privacy, budget caveat, exit code 4.

## Tests

- Unit: append/read round trip; corrupt lines skipped; totals by day/model; budget
  window and multiplier; query hashing.
- Integration: budget refusal makes no request; `stats --json` sums runs; cache hit
  records `cost_usd = 0` and `cached = true`.

## Out of scope

- Cloud sync, team budgets, per-API-key budgets.
- Enforcing a hard cap mid-request.
- Automatic ledger rotation.
