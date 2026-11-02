# Response cache and offline replay

## Goal

Stop paying for identical searches and make agent retries cheap. X Search bills per
fetched post and does not deduplicate across requests; agents routinely re-run the same
query while iterating.

## Scope

- Cache successful results (including zero-post results) on disk.
- Cache key: schema/prompt version, base URL, normalized query, model, effort,
  `max_turns`, `limit`, filters, image/video flags.
- Default: read + write, 15 minute TTL.
- Cache hits work without `XAI_API_KEY`.
- Flags:
  - `--cache <on|off|refresh>` (default `on`)
  - `--no-cache` (alias for `--cache off`)
  - `--refresh` (alias for `--cache refresh`)
  - `--cache-ttl <duration>` such as `15m`, `1h`, `7d`
  - `--cache-dir <path>`
  - `--replay <file>` to render a raw response or cache entry offline
- Environment: `XAVIER_CACHE_DIR`, otherwise XDG cache (`~/Library/Caches/xavier` on
  macOS, `%LOCALAPPDATA%\xavier` on Windows).
- Output: add `meta.cache = {hit, age_seconds, saved_usd}`; Markdown gets `cache=hit`.
- Ignore corrupt or expired entries; write atomically with `0600` permissions.
- `--raw` runs may read a cache entry but do not write one.

## Implementation sketch

- New `src/cache.rs`: key hashing (`sha2`), path resolution, load/store, replay parser.
- `src/types.rs`: `CacheMeta` and `Option<CacheMeta>` on `Meta`.
- `src/args.rs`: `CacheMode`, new flags, duration parsing.
- `src/cli.rs`: resolve base URL early; replay; cache lookup before auth; store after
  `build_result`; verbose diagnostics.
- `src/output.rs`: cache metadata in Markdown and JSON.
- `src/dates.rs`: `parse_duration_seconds`.
- README: cache semantics, privacy, flags, architecture entry.

## Tests

- Unit: key changes with query/filter/base URL; TTL parsing; store/load round trip;
  expired entries removed; refresh/off skip reads; replay cache entry.
- Integration: second run hits cache and needs no key; `--refresh` bypasses; `--cache off`
  bypasses; `--replay` makes no request; `--raw` can read a cached payload.

## Out of scope

- Cache pruning/`xavier cache` subcommand.
- Cross-model or cross-backend sharing.
- Encrypting cached post text at rest.
