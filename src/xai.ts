import { toExclusiveDate, type Options } from "./args";
import { estimateCost, roundUsd } from "./cost";
import { isValidDateString } from "./dates";
import { XavierError } from "./errors";
import type { Meta, Post, SearchCall, SearchResult, Usage } from "./types";

export const API_BASE = "https://api.x.ai/v1";
const TICKS_PER_USD = 10_000_000_000;

const SYSTEM_PROMPT = `You are xavier, a search engine for X (Twitter) posts.

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
- Respect the requested date range and authors; the tool parameters also enforce them.`;

export interface SearchDeps {
  fetch?: typeof fetch;
  now?: () => number;
  baseUrl?: string;
}

export interface RawResponse {
  raw: unknown;
  elapsedMs: number;
}

export function buildRequestBody(options: Options): Record<string, unknown> {
  const tool: Record<string, unknown> = { type: "x_search" };
  if (options.handles.length > 0) tool.allowed_x_handles = options.handles;
  if (options.excludeHandles.length > 0) tool.excluded_x_handles = options.excludeHandles;
  if (options.from) tool.from_date = options.from;
  const toExclusive = toExclusiveDate(options.to);
  if (toExclusive) tool.to_date = toExclusive;
  if (options.images) tool.enable_image_understanding = true;
  if (options.videos) tool.enable_video_understanding = true;

  const body: Record<string, unknown> = {
    model: options.model,
    store: false,
    input: [
      { role: "system", content: SYSTEM_PROMPT },
      { role: "user", content: buildUserPrompt(options) },
    ],
    tools: [tool],
    text: {
      format: {
        type: "json_schema",
        name: "xavier_results",
        strict: true,
        schema: resultSchema(options.limit),
      },
    },
    max_turns: options.maxTurns,
    include: ["no_inline_citations"],
  };

  if (supportsReasoningEffort(options.model)) {
    body.reasoning = { effort: options.effort };
  }

  return body;
}

export function buildUserPrompt(options: Options): string {
  const lines = [`Search X for: ${options.query}`, "", "Constraints:"];
  lines.push(`- Return at most ${options.limit} posts.`);

  if (options.from && options.to) {
    lines.push(`- Posted from ${options.from} to ${options.to} inclusive (UTC).`);
  } else if (options.from) {
    lines.push(`- Posted on or after ${options.from} (UTC).`);
  } else if (options.to) {
    lines.push(`- Posted on or before ${options.to} (UTC).`);
  }

  if (options.handles.length > 0) {
    lines.push(`- Only these authors: ${options.handles.map((h) => `@${h}`).join(", ")}.`);
  }
  if (options.excludeHandles.length > 0) {
    lines.push(
      `- Exclude these authors: ${options.excludeHandles.map((h) => `@${h}`).join(", ")}.`,
    );
  }
  if (options.sort === "latest") {
    lines.push("- Prefer the newest posts; use Latest mode for keyword searches.");
  } else if (options.sort === "top") {
    lines.push("- Prefer highly engaged, relevant posts; use Top mode for keyword searches.");
  }

  return lines.join("\n");
}

function supportsReasoningEffort(model: string): boolean {
  return /^grok-4\.(5|6|7)(?:$|-)/.test(model) || model.startsWith("grok-4.20");
}

function resultSchema(limit: number): Record<string, unknown> {
  return {
    type: "object",
    properties: {
      summary: {
        type: "string",
        description: "2-4 factual sentences synthesizing the matching posts.",
      },
      posts: {
        type: "array",
        maxItems: limit,
        items: {
          type: "object",
          properties: {
            url: {
              type: "string",
              format: "uri",
              description: "Exact X post status URL.",
            },
            author: {
              type: "string",
              description: "Author handle without the leading @.",
            },
            date: {
              type: ["string", "null"],
              description: "UTC date as YYYY-MM-DD, or null if unknown.",
            },
            excerpt: {
              type: "string",
              description: "Verbatim excerpt of the post, at most 280 characters.",
            },
          },
          required: ["url", "author", "date", "excerpt"],
          additionalProperties: false,
        },
      },
    },
    required: ["summary", "posts"],
    additionalProperties: false,
  };
}

export async function requestXPosts(
  options: Options,
  apiKey: string,
  deps: SearchDeps = {},
): Promise<RawResponse> {
  const fetchImpl = deps.fetch ?? fetch;
  const clock = deps.now ?? (() => performance.now());
  const started = clock();
  const baseUrl = (deps.baseUrl ?? process.env.XAI_BASE_URL ?? API_BASE).replace(/\/+$/, "");
  const url = `${baseUrl}/responses`;
  const body = buildRequestBody(options);

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), options.timeoutMs);

  let response: Response;
  let text: string;
  try {
    response = await fetchImpl(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
        Authorization: `Bearer ${apiKey}`,
      },
      body: JSON.stringify(body),
      signal: controller.signal,
    });
    text = await response.text();
  } catch (err) {
    if (controller.signal.aborted) {
      throw new XavierError(
        `Request timed out after ${Math.round(options.timeoutMs / 1000)}s.`,
        { hint: "Raise --timeout or narrow the search." },
      );
    }
    const message = err instanceof Error ? err.message : String(err);
    throw new XavierError(`Could not reach xAI: ${message}`, {
      hint: `Check network access to ${baseUrl}.`,
    });
  } finally {
    clearTimeout(timer);
  }

  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    if (!response.ok) throw apiError(response.status, null, text);
    throw new XavierError(`xAI returned non-JSON (HTTP ${response.status}).`, {
      hint: text.slice(0, 300) || undefined,
    });
  }

  if (!response.ok) throw apiError(response.status, data, text);

  const elapsedMs = Math.max(0, Math.round(clock() - started));
  return { raw: data, elapsedMs };
}

export function buildResult(data: unknown, options: Options, elapsedMs: number): SearchResult {
  const root = asRecord(data) ?? {};

  if (root.error != null) {
    throw new XavierError(`xAI API error: ${errorMessage(root.error)}`);
  }

  const status = asString(root.status);
  if (status === "incomplete") {
    const reason = asString(asRecord(root.incomplete_details)?.reason) ?? "unknown reason";
    throw new XavierError(`xAI response was incomplete: ${reason}.`, {
      hint: "Raise --max-turns, narrow the query, or retry.",
    });
  }
  if (status === "failed") {
    throw new XavierError("xAI response failed.", {
      hint: "Retry, or use --raw to inspect the payload.",
    });
  }

  const outputText = extractOutputText(root);
  if (outputText == null) {
    throw new XavierError("xAI returned no final message.", {
      hint: "Retry, raise --max-turns, or use --raw to inspect the payload.",
    });
  }

  const parsed = asRecord(parseJsonText(outputText));
  if (parsed == null) {
    throw new XavierError("Could not parse the structured result from xAI.", {
      hint: "Retry with a narrower query, or use --raw to inspect the payload.",
    });
  }

  const summary = (asString(parsed.summary) ?? "").trim();
  const warnings: string[] = [];

  let posts = dedupePosts(asArray(parsed.posts).map(parsePost).filter(isPost));

  const citedIds = collectCitationIds(root);
  if (posts.length > 0) {
    if (citedIds.size === 0) {
      warnings.push("No X citation URLs were returned; posts are unverified.");
    } else {
      const kept = posts.filter((post) => citedIds.has(post.id));
      const dropped = posts.length - kept.length;
      if (dropped > 0) {
        warnings.push(`Dropped ${dropped} post(s) that did not appear in X Search citations.`);
      }
      for (const post of kept) post.verified = true;
      posts = kept;
    }
  }

  if (posts.length > options.limit) {
    posts = posts.slice(0, options.limit);
    warnings.push(`Truncated to ${options.limit} posts.`);
  }

  const usage = extractUsage(root, options.model, warnings);
  const searches = collectSearches(root);

  const trimmedSummary = summary.trim();
  const isStub = trimmedSummary === "" || /^(placeholder|todo|tbd|n\/?a)$/i.test(trimmedSummary);
  if (posts.length === 0 && isStub && usage.x_posts_fetched > 0) {
    throw new XavierError(
      `xAI returned a placeholder result after fetching ${usage.x_posts_fetched} posts.`,
      {
        hint: "This usually means --max-turns was exhausted. Retry with a higher --max-turns or a narrower query.",
      },
    );
  }

  const responseId = asString(root.id);
  const model = asString(root.model) ?? options.model;
  const createdAt = unixToIso(asNumber(root.created_at)) ?? new Date().toISOString();

  const meta: Meta = {
    model,
    response_id: responseId,
    created_at: createdAt,
    elapsed_ms: elapsedMs,
    effort: options.effort,
    max_turns: options.maxTurns,
    filters: {
      from: options.from,
      to: options.to,
      handles: options.handles,
      exclude_handles: options.excludeHandles,
      limit: options.limit,
      sort: options.sort,
    },
    usage,
    searches,
    citations: citedIds.size,
    warnings,
  };

  return { query: options.query, summary, posts, meta };
}

function extractOutputText(root: Record<string, unknown>): string | null {
  let text: string | null = null;
  for (const item of asArray(root.output)) {
    const message = asRecord(item);
    if (asString(message?.type) !== "message") continue;
    for (const content of asArray(message?.content)) {
      const block = asRecord(content);
      if (asString(block?.type) !== "output_text") continue;
      const candidate = asString(block?.text);
      if (candidate != null && candidate.trim() !== "") text = candidate;
    }
  }
  return text;
}

function parseJsonText(text: string): unknown | null {
  const trimmed = text.trim();
  try {
    return JSON.parse(trimmed);
  } catch {
    // fall through
  }

  const fenced = /```(?:json)?\s*([\s\S]*?)```/i.exec(trimmed);
  if (fenced?.[1]) {
    try {
      return JSON.parse(fenced[1].trim());
    } catch {
      // fall through
    }
  }

  const start = trimmed.indexOf("{");
  const end = trimmed.lastIndexOf("}");
  if (start >= 0 && end > start) {
    try {
      return JSON.parse(trimmed.slice(start, end + 1));
    } catch {
      // fall through
    }
  }

  return null;
}

interface CanonicalUrl {
  url: string;
  id: string;
  handle: string | null;
}

function canonicalPostUrl(raw: string): CanonicalUrl | null {
  let value = raw.trim();
  if (/^(?:www\.)?(?:x|twitter)\.com\//i.test(value)) value = `https://${value}`;

  const match =
    /^https?:\/\/(?:www\.)?(?:x|twitter)\.com\/([A-Za-z0-9_]{1,15}|i)\/(?:web\/)?status(?:es)?\/(\d{1,25})(?:[/?#].*)?$/i.exec(
      value,
    );
  if (!match) return null;

  const handle = match[1];
  const id = match[2];
  if (!handle || !id) return null;

  const isBare = handle.toLowerCase() === "i";
  return {
    url: isBare ? `https://x.com/i/status/${id}` : `https://x.com/${handle}/status/${id}`,
    id,
    handle: isBare ? null : handle,
  };
}

function parsePost(value: unknown): Post | null {
  const item = asRecord(value);
  if (!item) return null;

  const rawUrl = asString(item.url);
  if (!rawUrl) return null;
  const canonical = canonicalPostUrl(rawUrl);
  if (!canonical) return null;

  const rawAuthor = (asString(item.author) ?? "").trim().replace(/^@+/, "");
  const author = /^[A-Za-z0-9_]{1,15}$/.test(rawAuthor)
    ? rawAuthor
    : (canonical.handle ?? "unknown");

  const rawDate = asString(item.date);
  const date = rawDate != null && isValidDateString(rawDate) ? rawDate : null;
  const excerpt = (asString(item.excerpt) ?? "").replace(/\s+/g, " ").trim();

  return {
    url: canonical.url,
    id: canonical.id,
    author,
    date,
    excerpt,
    verified: false,
  };
}

function isPost(value: Post | null): value is Post {
  return value != null;
}

function dedupePosts(posts: Post[]): Post[] {
  const seen = new Set<string>();
  const out: Post[] = [];
  for (const post of posts) {
    if (seen.has(post.id)) continue;
    seen.add(post.id);
    out.push(post);
  }
  return out;
}

function collectCitationIds(root: Record<string, unknown>): Set<string> {
  const ids = new Set<string>();
  const add = (value: unknown) => {
    if (typeof value !== "string") return;
    const canonical = canonicalPostUrl(value);
    if (canonical) ids.add(canonical.id);
  };

  for (const citation of asArray(root.citations)) {
    if (typeof citation === "string") add(citation);
    else add(asString(asRecord(citation)?.url));
  }

  for (const item of asArray(root.output)) {
    const record = asRecord(item);
    if (!record) continue;
    for (const annotation of asArray(record.annotations)) {
      const entry = asRecord(annotation);
      if (asString(entry?.type) === "url_citation") add(entry?.url);
    }
    for (const content of asArray(record.content)) {
      for (const annotation of asArray(asRecord(content)?.annotations)) {
        const entry = asRecord(annotation);
        if (asString(entry?.type) === "url_citation") add(entry?.url);
      }
    }
  }

  return ids;
}

function collectSearches(root: Record<string, unknown>): SearchCall[] {
  const out: SearchCall[] = [];
  for (const item of asArray(root.output)) {
    const record = asRecord(item);
    if (asString(record?.type) !== "custom_tool_call") continue;
    const name = asString(record?.name);
    if (!name || !name.startsWith("x_")) continue;

    let input: unknown = record?.input;
    if (typeof input === "string") {
      try {
        input = JSON.parse(input);
      } catch {
        input = null;
      }
    }
    const args = asRecord(input);
    out.push({
      name,
      query: asString(args?.query),
      mode: asString(args?.mode),
    });
  }
  return out;
}

function extractUsage(
  root: Record<string, unknown>,
  model: string,
  warnings: string[],
): Usage {
  const usage = asRecord(root.usage) ?? {};
  const inputDetails = asRecord(usage.input_tokens_details) ?? {};
  const outputDetails = asRecord(usage.output_tokens_details) ?? {};
  const tools = asRecord(usage.server_side_tool_usage_details) ?? {};

  const input = asNumber(usage.input_tokens) ?? 0;
  const cached = asNumber(inputDetails.cached_tokens) ?? 0;
  const output = asNumber(usage.output_tokens) ?? 0;
  const reasoning = asNumber(outputDetails.reasoning_tokens) ?? 0;
  const total = asNumber(usage.total_tokens) ?? input + output;
  const xCalls = asNumber(tools.x_search_calls) ?? 0;
  const xPosts = asNumber(tools.x_posts_fetched) ?? 0;
  const xUsers = asNumber(tools.x_users_fetched) ?? 0;

  const ticks = asNumber(usage.cost_in_usd_ticks);
  let costUsd: number | null = null;
  let costSource: Usage["cost_source"] = null;

  if (ticks != null) {
    costUsd = roundUsd(ticks / TICKS_PER_USD);
    costSource = "api";
  } else {
    const estimate = estimateCost({ model, input, cached, output, xPosts, xUsers });
    if (estimate != null) {
      costUsd = estimate;
      costSource = "estimated";
      warnings.push("xAI did not return an exact cost; cost_usd is an estimate.");
    }
  }

  return {
    input_tokens: input,
    cached_input_tokens: cached,
    output_tokens: output,
    reasoning_tokens: reasoning,
    total_tokens: total,
    x_search_calls: xCalls,
    x_posts_fetched: xPosts,
    x_users_fetched: xUsers,
    cost_usd: costUsd,
    cost_source: costSource,
  };
}

function apiError(status: number, data: unknown, text: string): XavierError {
  const root = asRecord(data);
  const message =
    errorMessage(root?.error) ??
    (text.trim() !== "" ? text.slice(0, 500) : `HTTP ${status}`);

  let hint: string | undefined;
  if (status === 401) {
    hint = "Check XAI_API_KEY; try `doppler run -- xavier ...`.";
  } else if (status === 403) {
    hint = "This key may not have access to the model or the x_search tool.";
  } else if (status === 404) {
    hint = "Model or endpoint not found. Check --model.";
  } else if (status === 429) {
    hint = "Rate limited; wait and retry.";
  } else if (status >= 500) {
    hint = "xAI service error; retry shortly.";
  }

  return new XavierError(`xAI API error ${status}: ${message}`, { hint });
}

function errorMessage(error: unknown): string | null {
  return (
    asString(asRecord(error)?.message) ??
    asString(error) ??
    (error == null ? null : JSON.stringify(error).slice(0, 500))
  );
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return null;
}

function unixToIso(seconds: number | null): string | null {
  if (seconds == null) return null;
  const date = new Date(seconds * 1000);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}
