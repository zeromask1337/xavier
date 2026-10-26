import { parseArgs } from "node:util";
import { addDays, parseWhen } from "./dates";
import { UsageError } from "./errors";

export const DEFAULT_MODEL = "grok-4.7";
export const DEFAULT_LIMIT = 20;
export const DEFAULT_MAX_TURNS = 3;
export const DEFAULT_TIMEOUT_SECONDS = 180;

export type Effort = "low" | "medium" | "high" | "xhigh";
export type SortMode = "latest" | "top";
export type OutputFormat = "md" | "json" | "jsonl" | "raw";

export interface Options {
  query: string;
  format: OutputFormat;
  model: string;
  handles: string[];
  excludeHandles: string[];
  from: string | null;
  to: string | null;
  limit: number;
  maxTurns: number;
  effort: Effort;
  sort: SortMode | null;
  images: boolean;
  videos: boolean;
  timeoutMs: number;
  dryRun: boolean;
  verbose: boolean;
}

export type ParsedCli =
  | { kind: "help" }
  | { kind: "version" }
  | { kind: "run"; options: Options };

type Env = Record<string, string | undefined>;

const HANDLE_RE = /^[A-Za-z0-9_]{1,15}$/;

export function parseCli(
  argv: string[],
  env: Env = process.env,
  now: Date = new Date(),
): ParsedCli {
  let values: {
    help?: boolean;
    version?: boolean;
    json?: boolean;
    jsonl?: boolean;
    md?: boolean;
    raw?: boolean;
    model?: string;
    handle?: string[];
    "exclude-handle"?: string[];
    from?: string;
    since?: string;
    to?: string;
    until?: string;
    limit?: string;
    "max-turns"?: string;
    effort?: string;
    sort?: string;
    images?: boolean;
    videos?: boolean;
    timeout?: string;
    "dry-run"?: boolean;
    verbose?: boolean;
  };
  let positionals: string[];

  try {
    const parsed = parseArgs({
      args: argv,
      allowPositionals: true,
      strict: true,
      options: {
        help: { type: "boolean", short: "h" },
        version: { type: "boolean", short: "V" },
        json: { type: "boolean" },
        jsonl: { type: "boolean" },
        md: { type: "boolean" },
        raw: { type: "boolean" },
        model: { type: "string" },
        handle: { type: "string", short: "u", multiple: true },
        "exclude-handle": { type: "string", short: "x", multiple: true },
        from: { type: "string" },
        since: { type: "string" },
        to: { type: "string" },
        until: { type: "string" },
        limit: { type: "string", short: "n" },
        "max-turns": { type: "string" },
        effort: { type: "string" },
        sort: { type: "string" },
        images: { type: "boolean" },
        videos: { type: "boolean" },
        timeout: { type: "string" },
        "dry-run": { type: "boolean" },
        verbose: { type: "boolean", short: "v" },
      },
    });
    values = parsed.values as typeof values;
    positionals = parsed.positionals;
  } catch (err) {
    throw new UsageError(err instanceof Error ? err.message : String(err));
  }

  if (values.help) return { kind: "help" };
  if (values.version) return { kind: "version" };

  const args = [...positionals];
  const head = args[0];
  if (head === "help") return { kind: "help" };
  if (head === "version") return { kind: "version" };
  if (head === "search" || head === "s") args.shift();

  const query = args.join(" ").trim();

  const selected: OutputFormat[] = [];
  if (values.json) selected.push("json");
  if (values.jsonl) selected.push("jsonl");
  if (values.md) selected.push("md");
  if (values.raw) selected.push("raw");
  if (selected.length > 1) {
    throw new UsageError(
      `Choose one output format, not ${selected.map((f) => `--${f}`).join(" and ")}.`,
    );
  }
  const format: OutputFormat = selected[0] ?? "md";

  const handles = collectHandles(values.handle, "--handle");
  const excludeHandles = collectHandles(values["exclude-handle"], "--exclude-handle");
  if (handles.length > 0 && excludeHandles.length > 0) {
    throw new UsageError("Use either --handle or --exclude-handle, not both.");
  }

  const fromRaw = pickOne(values.from, values.since, "--from", "--since");
  const toRaw = pickOne(values.to, values.until, "--to", "--until");
  const from = fromRaw == null ? null : parseWhen(fromRaw, now);
  const to = toRaw == null ? null : parseWhen(toRaw, now);
  if (from != null && to != null && from > to) {
    throw new UsageError(`--from (${from}) must not be after --to (${to}).`);
  }

  const limit = intOption(values.limit, "--limit", DEFAULT_LIMIT, 1, 100);
  const maxTurns = intOption(values["max-turns"], "--max-turns", DEFAULT_MAX_TURNS, 1, 10);
  const timeoutSeconds = intOption(
    values.timeout,
    "--timeout",
    DEFAULT_TIMEOUT_SECONDS,
    5,
    600,
  );

  const model = (values.model ?? env.XAVIER_MODEL ?? DEFAULT_MODEL).trim();
  if (!model || /\s/.test(model)) {
    throw new UsageError(`Invalid --model value "${model}".`);
  }

  const effort = parseEffort(values.effort);
  const sort = parseSort(values.sort);

  return {
    kind: "run",
    options: {
      query,
      format,
      model,
      handles,
      excludeHandles,
      from,
      to,
      limit,
      maxTurns,
      effort,
      sort,
      images: Boolean(values.images),
      videos: Boolean(values.videos),
      timeoutMs: timeoutSeconds * 1000,
      dryRun: Boolean(values["dry-run"]),
      verbose: Boolean(values.verbose),
    },
  };
}

/** Inclusive calendar end date => exclusive xAI `to_date`. */
export function toExclusiveDate(to: string | null): string | null {
  return to == null ? null : addDays(to, 1);
}

function collectHandles(raw: string[] | undefined, flag: string): string[] {
  const out: string[] = [];
  for (const chunk of raw ?? []) {
    for (const part of chunk.split(",")) {
      const handle = part.trim().replace(/^@+/, "");
      if (!handle) continue;
      if (!HANDLE_RE.test(handle)) {
        throw new UsageError(`${flag}: "${part.trim()}" is not a valid X handle.`);
      }
      if (!out.some((existing) => existing.toLowerCase() === handle.toLowerCase())) {
        out.push(handle);
      }
    }
  }
  if (out.length > 20) throw new UsageError(`${flag}: at most 20 handles are allowed.`);
  return out;
}

function pickOne(
  first: string | undefined,
  second: string | undefined,
  firstName: string,
  secondName: string,
): string | undefined {
  if (first != null && second != null) {
    throw new UsageError(`Use only one of ${firstName} and ${secondName}.`);
  }
  return first ?? second;
}

function intOption(
  raw: string | undefined,
  flag: string,
  fallback: number,
  min: number,
  max: number,
): number {
  if (raw == null) return fallback;
  if (!/^\d+$/.test(raw.trim())) {
    throw new UsageError(`${flag} must be a whole number between ${min} and ${max}.`);
  }
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    throw new UsageError(`${flag} must be between ${min} and ${max}.`);
  }
  return value;
}

function parseEffort(raw: string | undefined): Effort {
  if (raw == null) return "low";
  const value = raw.trim().toLowerCase();
  if (value === "low" || value === "medium" || value === "high" || value === "xhigh") {
    return value;
  }
  throw new UsageError(`--effort must be low, medium, high, or xhigh.`);
}

function parseSort(raw: string | undefined): SortMode | null {
  if (raw == null) return null;
  const value = raw.trim().toLowerCase();
  if (value === "latest" || value === "recent") return "latest";
  if (value === "top" || value === "relevance" || value === "relevant") return "top";
  throw new UsageError(`--sort must be latest or top.`);
}

export function helpText(): string {
  return `xavier — search X posts from the terminal

USAGE
  xavier [search] <query> [options]
  echo "<query>" | xavier -
  xavier --help
  xavier --version

QUERY
  Natural language, or X search syntax such as:
    from:xai "Grok 4.7" min_likes:50 -is:retweet
  The backend turns it into one or more X searches.

OUTPUT
  Default      Compact Markdown (stable headings + key=value metadata)
  --json       One structured JSON result object
  --jsonl      Result metadata line, then one post per line
  --raw        Raw xAI Responses API payload
  --dry-run    Print the API request body and exit without spending credits

FILTERS
  -u, --handle <handle>          Only these authors (repeatable, comma-separated, max 20)
  -x, --exclude-handle <handle>  Exclude these authors (repeatable, comma-separated, max 20)
      --from, --since <when>     Start date: YYYY-MM-DD, 7d, 24h, today
      --to, --until <when>       End date, inclusive: YYYY-MM-DD, today
  -n, --limit <n>                Max posts to return (1-100, default 20)
      --sort <latest|top>        Ranking hint
      --max-turns <n>            Max agent turns (1-10, default ${DEFAULT_MAX_TURNS})
      --effort <level>           low | medium | high | xhigh (Grok 4.5+, default low)
      --images                   Let the model view images inside posts
      --videos                   Let the model watch videos inside posts

OTHER
      --model <id>               xAI model (default ${DEFAULT_MODEL})
      --timeout <seconds>        Request timeout (5-600, default ${DEFAULT_TIMEOUT_SECONDS})
  -v, --verbose                  Diagnostics on stderr

AUTH
  XAI_API_KEY, for example:
    doppler run -- xavier "what are people saying about Grok 4.7"

EXAMPLES
  xavier "what are people saying about Grok 4.7"
  xavier search "rust async" --from 7d --limit 10 --json
  xavier "from:xai min_likes:100" --sort top
  xavier - < query.txt
`;
}
