import { describe, expect, test } from "bun:test";
import type { Options } from "../src/args";
import { XavierError } from "../src/errors";
import { buildRequestBody, buildResult, buildUserPrompt, requestXPosts } from "../src/xai";
import fixture from "./fixtures/response.json";

function options(overrides: Partial<Options> = {}): Options {
  return {
    query: "grok 4.7",
    format: "md",
    model: "grok-4.7",
    handles: [],
    excludeHandles: [],
    from: null,
    to: null,
    limit: 20,
    maxTurns: 2,
    effort: "low",
    sort: null,
    images: false,
    videos: false,
    timeoutMs: 180_000,
    dryRun: false,
    verbose: false,
    ...overrides,
  };
}

describe("buildRequestBody", () => {
  test("builds a bounded, non-stored X Search request", () => {
    const body = buildRequestBody(options({ limit: 5 })) as any;
    expect(body.model).toBe("grok-4.7");
    expect(body.store).toBe(false);
    expect(body.max_turns).toBe(2);
    expect(body.reasoning).toEqual({ effort: "low" });
    expect(body.include).toEqual(["no_inline_citations"]);
    expect(body.tools).toEqual([{ type: "x_search" }]);
    expect(body.text.format.type).toBe("json_schema");
    expect(body.text.format.strict).toBe(true);
    expect(body.text.format.schema.properties.posts.maxItems).toBe(5);
    expect(body.input[0].role).toBe("system");
    expect(body.input[1].content).toContain("Search X for: grok 4.7");
  });

  test("maps filters to X Search tool parameters", () => {
    const body = buildRequestBody(
      options({
        handles: ["alice", "bob"],
        from: "2026-10-18",
        to: "2026-10-25",
        images: true,
        videos: true,
        sort: "latest",
      }),
    ) as any;
    expect(body.tools[0]).toEqual({
      type: "x_search",
      allowed_x_handles: ["alice", "bob"],
      from_date: "2026-10-18",
      to_date: "2026-10-26",
      enable_image_understanding: true,
      enable_video_understanding: true,
    });
    expect(body.input[1].content).toContain("Only these authors: @alice, @bob");
    expect(body.input[1].content).toContain("Latest mode");
  });

  test("supports excluded handles without allow lists", () => {
    const body = buildRequestBody(options({ excludeHandles: ["carol"] })) as any;
    expect(body.tools[0].excluded_x_handles).toEqual(["carol"]);
    expect(body.tools[0].allowed_x_handles).toBeUndefined();
  });

  test("omits reasoning for models without effort support", () => {
    const body = buildRequestBody(options({ model: "grok-4.3" })) as any;
    expect(body.reasoning).toBeUndefined();
  });
});

describe("buildUserPrompt", () => {
  test("states limits and range", () => {
    const prompt = buildUserPrompt(
      options({ limit: 7, from: "2026-10-01", to: "2026-10-25" }),
    );
    expect(prompt).toContain("Return at most 7 posts.");
    expect(prompt).toContain("from 2026-10-01 to 2026-10-25 inclusive");
  });
});

describe("requestXPosts", () => {
  test("posts to the configured base URL and returns the raw payload", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const mockFetch = (async (url: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(url), init });
      return new Response(JSON.stringify(fixture), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }) as unknown as typeof fetch;

    let tick = 0;
    const { raw, elapsedMs } = await requestXPosts(options(), "xai-test", {
      fetch: mockFetch,
      baseUrl: "https://api.test/v1/",
      now: () => (tick += 100),
    });

    expect(raw).toEqual(fixture);
    expect(elapsedMs).toBe(100);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe("https://api.test/v1/responses");

    const headers = calls[0]?.init?.headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer xai-test");
    const body = JSON.parse(String(calls[0]?.init?.body));
    expect(body.model).toBe("grok-4.7");
    expect(body.tools[0].type).toBe("x_search");
  });

  test("surfaces API errors with a hint", async () => {
    const mockFetch = (async () =>
      new Response(JSON.stringify({ error: { message: "invalid key" } }), {
        status: 401,
      })) as unknown as typeof fetch;

    try {
      await requestXPosts(options(), "bad-key", { fetch: mockFetch });
      throw new Error("expected requestXPosts to throw");
    } catch (err) {
      expect(err).toBeInstanceOf(XavierError);
      expect((err as XavierError).message).toContain("invalid key");
      expect((err as XavierError).hint).toContain("XAI_API_KEY");
    }
  });

  test("keeps raw payload available when structured parsing would fail", async () => {
    const stub = structuredClone(fixture) as any;
    stub.output[2].content[0].text = '{"summary":"placeholder","posts":[]}';
    const mockFetch = (async () =>
      new Response(JSON.stringify(stub), { status: 200 })) as unknown as typeof fetch;

    const { raw } = await requestXPosts(options(), "key", { fetch: mockFetch });
    expect(() => buildResult(raw, options(), 0)).toThrow("placeholder result");
  });
});

describe("buildResult", () => {
  test("validates, de-duplicates, and verifies cited posts", () => {
    const result = buildResult(fixture, options(), 1234);

    expect(result.query).toBe("grok 4.7");
    expect(result.summary).toBe("Mixed reactions.");
    expect(result.posts).toHaveLength(2);

    expect(result.posts[0]).toEqual({
      url: "https://x.com/alice/status/111",
      id: "111",
      author: "alice",
      date: "2026-10-24",
      excerpt: "Grok 4.7 is great.",
      verified: true,
    });
    expect(result.posts[1]?.id).toBe("222");
    expect(result.posts[1]?.date).toBeNull();

    expect(result.meta.model).toBe("grok-4.7");
    expect(result.meta.response_id).toBe("resp_test_123");
    expect(result.meta.citations).toBe(3);
    expect(result.meta.elapsed_ms).toBe(1234);
    expect(result.meta.warnings).toEqual([
      "Dropped 1 post(s) that did not appear in X Search citations.",
    ]);
    expect(result.meta.searches).toEqual([
      { name: "x_keyword_search", query: "grok 4.7", mode: "Latest" },
      { name: "x_semantic_search", query: "grok discussion", mode: null },
    ]);
  });

  test("uses the exact cost returned by the API", () => {
    const result = buildResult(fixture, options(), 0);
    expect(result.meta.usage.cost_usd).toBe(0.001234);
    expect(result.meta.usage.cost_source).toBe("api");
    expect(result.meta.usage.x_posts_fetched).toBe(3);
    expect(result.meta.usage.x_search_calls).toBe(2);
    expect(result.meta.usage.cached_input_tokens).toBe(100);
  });

  test("keeps posts but flags them when no citations are returned", () => {
    const data = structuredClone(fixture) as any;
    delete data.output[2].content[0].annotations;

    const result = buildResult(data, options(), 0);
    expect(result.posts).toHaveLength(3);
    expect(result.posts.every((post) => post.verified === false)).toBe(true);
    expect(result.meta.warnings).toContain(
      "No X citation URLs were returned; posts are unverified.",
    );
  });

  test("drops posts that do not match returned X citations", () => {
    const data = structuredClone(fixture) as any;
    data.output[2].content[0].annotations = [
      { type: "url_citation", url: "https://x.com/i/status/999" },
    ];

    const result = buildResult(data, options(), 0);
    expect(result.posts).toHaveLength(0);
    expect(result.meta.warnings).toContain(
      "Dropped 3 post(s) that did not appear in X Search citations.",
    );
  });

  test("enforces the limit", () => {
    const result = buildResult(fixture, options({ limit: 1 }), 0);
    expect(result.posts).toHaveLength(1);
    expect(result.meta.warnings).toContain("Truncated to 1 posts.");
  });

  test("rejects responses without a final message", () => {
    expect(() => buildResult({ output: [] }, options(), 0)).toThrow(XavierError);
  });

  test("rejects unparseable structured output", () => {
    const data = structuredClone(fixture) as any;
    data.output[2].content[0].text = "not json";
    expect(() => buildResult(data, options(), 0)).toThrow(XavierError);
  });

  test("rejects placeholder stubs after a search was performed", () => {
    const data = structuredClone(fixture) as any;
    data.output[2].content[0].text = '{"summary":"placeholder","posts":[]}';
    expect(() => buildResult(data, options(), 0)).toThrow("placeholder result");
  });

  test("rejects incomplete responses", () => {
    const data = {
      ...structuredClone(fixture),
      status: "incomplete",
      incomplete_details: { reason: "max_turns" },
      output: [],
    };
    expect(() => buildResult(data, options(), 0)).toThrow("incomplete");
  });
});
