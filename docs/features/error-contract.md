# Machine-readable errors, schema version, and capabilities

## Goal

Turn xavier from a CLI that agents shell out to into a tool agents can introspect and
retry without parsing English stderr.

## Scope

- Add `ErrorCode` to `AppError` with a stable string form:
  `usage`, `missing_api_key`, `auth`, `forbidden`, `not_found`, `rate_limited`,
  `server`, `timeout`, `network`, `invalid_response`, `incomplete`, `placeholder`,
  `budget_exceeded`, `empty`.
- Add `retryable: bool` and optional `http_status` to errors.
- `--json-errors` emits one JSON object to stderr:
  `{"error":{"code":"rate_limited","message":"...","hint":"...","retryable":true,"http_status":429}}`
- Preserve existing exit codes: `0` success, `1` runtime, `2` usage. Reserve `4` for
  budget (ledger PR).
- Add `schema_version: 1` to JSON and the JSONL result line.
- `xavier capabilities --json`: name, version, schema version, commands, flags,
  defaults, limits, output formats, environment variables.
- `xavier schema output`: JSON Schema for the `SearchResult` object.
- Document the error codes and retry guidance in the README.

## Implementation sketch

- `src/errors.rs`: `ErrorCode`, constructors, `retryable`, `http_status`, serialization.
- `src/xai.rs`: map HTTP status and response failures to codes.
- `src/cli.rs`: `report` chooses prose or JSON based on `--json-errors`; subcommand
  dispatch for `capabilities` and `schema`.
- `src/args.rs`: flags and `ParsedCli` variants.
- New `src/capabilities.rs`: manifest and output JSON Schema (static, generated from
  the existing result types where practical).
- README: error table, `--json-errors`, introspection commands.

## Tests

- Unit: every `AppError` maps to the expected code/retryable; HTTP 401/403/404/429/5xx
  mapping.
- Integration: `--json-errors` on usage, auth, and API failures; stdout remains empty;
  `capabilities --json` parses and contains the schema version; `schema output` parses
  as JSON Schema; JSON output includes `schema_version`.

## Out of scope

- MCP server mode.
- OpenAPI document.
- Changing existing exit-code meanings.
