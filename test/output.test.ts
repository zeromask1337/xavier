import { describe, expect, test } from "bun:test";
import type { Options } from "../src/args";
import { renderJson, renderJsonl, renderMarkdown } from "../src/output";
import { buildResult } from "../src/xai";
import fixture from "./fixtures/response.json";

const options: Options = {
  query: "grok 4.7",
  format: "md",
  model: "grok-4.7",
  handles: ["alice"],
  excludeHandles: [],
  from: "2026-10-18",
  to: "2026-10-25",
  limit: 20,
  maxTurns: 2,
  effort: "low",
  sort: "latest",
  images: false,
  videos: false,
  timeoutMs: 180_000,
  dryRun: false,
  verbose: false,
};

const result = buildResult(fixture, options, 9300);

describe("renderMarkdown", () => {
  test("emits stable, agent-readable sections", () => {
    const markdown = renderMarkdown(result);
    expect(markdown).toContain("# X search: grok 4.7");
    expect(markdown).toContain(
      "model=grok-4.7 effort=low posts=2 fetched=3 searches=2 cost_usd=0.0012",
    );
    expect(markdown).toContain("from=2026-10-18 to=2026-10-25 handles=@alice sort=latest");
    expect(markdown).toContain("## Warnings");
    expect(markdown).toContain("## Summary");
    expect(markdown).toContain("## Posts");
    expect(markdown).toContain("1. @alice | 2026-10-24 | https://x.com/alice/status/111");
    expect(markdown).toContain("   > Grok 4.7 is great.");
  });

  test("renders an empty result", () => {
    const empty = {
      ...result,
      posts: [],
      meta: { ...result.meta, warnings: [] },
      summary: "",
    };
    expect(renderMarkdown(empty)).toContain("No summary returned.");
    expect(renderMarkdown(empty)).toContain("No matching posts.");
  });

  test("marks unverified posts", () => {
    const unverified = {
      ...result,
      posts: result.posts.map((post) => ({ ...post, verified: false })),
    };
    expect(renderMarkdown(unverified)).toContain("| unverified");
  });
});

describe("renderJson", () => {
  test("round-trips the result object", () => {
    const parsed = JSON.parse(renderJson(result));
    expect(parsed.query).toBe("grok 4.7");
    expect(parsed.posts).toHaveLength(2);
    expect(parsed.meta.usage.cost_usd).toBe(0.001234);
    expect(parsed.meta.usage.cost_source).toBe("api");
  });
});

describe("renderJsonl", () => {
  test("emits a result line followed by post lines", () => {
    const lines = renderJsonl(result).trimEnd().split("\n");
    expect(lines).toHaveLength(3);

    const head = JSON.parse(lines[0] ?? "");
    expect(head.type).toBe("result");
    expect(head.posts).toBeUndefined();
    expect(head.summary).toBe("Mixed reactions.");

    const post = JSON.parse(lines[1] ?? "");
    expect(post.type).toBe("post");
    expect(post.url).toBe("https://x.com/alice/status/111");
  });
});
