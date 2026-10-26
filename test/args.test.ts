import { describe, expect, test } from "bun:test";
import { parseCli, toExclusiveDate, type Options } from "../src/args";
import { UsageError } from "../src/errors";

const NOW = new Date("2026-10-25T12:00:00Z");
const ENV: Record<string, string | undefined> = {};

function run(argv: string[], env: Record<string, string | undefined> = ENV): Options {
  const parsed = parseCli(argv, env, NOW);
  if (parsed.kind !== "run") throw new Error(`expected run, got ${parsed.kind}`);
  return parsed.options;
}

describe("parseCli", () => {
  test("parses a bare query with defaults", () => {
    const options = run(["what are people saying about Grok 4.7"]);
    expect(options.query).toBe("what are people saying about Grok 4.7");
    expect(options.format).toBe("md");
    expect(options.model).toBe("grok-4.7");
    expect(options.limit).toBe(20);
    expect(options.maxTurns).toBe(3);
    expect(options.effort).toBe("low");
    expect(options.timeoutMs).toBe(180_000);
  });

  test("supports the search subcommand and joined positionals", () => {
    expect(run(["search", "grok", "4.7"]).query).toBe("grok 4.7");
    expect(run(["s", "grok"]).query).toBe("grok");
  });

  test("returns help and version", () => {
    expect(parseCli(["--help"], ENV, NOW).kind).toBe("help");
    expect(parseCli(["help"], ENV, NOW).kind).toBe("help");
    expect(parseCli(["--version"], ENV, NOW).kind).toBe("version");
    expect(parseCli(["version"], ENV, NOW).kind).toBe("version");
  });

  test("normalizes and de-duplicates handles", () => {
    const options = run(["-u", "@alice,bob", "--handle", "ALICE"]);
    expect(options.handles).toEqual(["alice", "bob"]);
    expect(run(["-x", "carol"]).excludeHandles).toEqual(["carol"]);
  });

  test("rejects too many handles and mixed allow/deny", () => {
    const many = Array.from({ length: 21 }, (_, i) => `user${i}`).join(",");
    expect(() => run(["--handle", many])).toThrow(UsageError);
    expect(() => run(["--handle", "alice", "--exclude-handle", "bob"])).toThrow(UsageError);
  });

  test("parses date filters and validates order", () => {
    const options = run(["--from", "7d", "--to", "today"]);
    expect(options.from).toBe("2026-10-18");
    expect(options.to).toBe("2026-10-25");
    expect(toExclusiveDate(options.to)).toBe("2026-10-26");
    expect(() => run(["--from", "2026-10-25", "--to", "2026-10-18"])).toThrow(UsageError);
    expect(() => run(["--from", "1d", "--since", "2d"])).toThrow(UsageError);
  });

  test("enforces numeric ranges", () => {
    expect(run(["--limit", "5"]).limit).toBe(5);
    expect(() => run(["--limit", "0"])).toThrow(UsageError);
    expect(() => run(["--limit", "101"])).toThrow(UsageError);
    expect(() => run(["--limit", "five"])).toThrow(UsageError);
    expect(() => run(["--max-turns", "11"])).toThrow(UsageError);
    expect(() => run(["--timeout", "4"])).toThrow(UsageError);
  });

  test("parses sort and effort aliases", () => {
    expect(run(["--sort", "recent"]).sort).toBe("latest");
    expect(run(["--sort", "relevance"]).sort).toBe("top");
    expect(run(["--effort", "xhigh"]).effort).toBe("xhigh");
    expect(() => run(["--sort", "random"])).toThrow(UsageError);
    expect(() => run(["--effort", "max"])).toThrow(UsageError);
  });

  test("rejects conflicting output formats", () => {
    expect(() => run(["--json", "--raw"])).toThrow(UsageError);
    expect(run(["--json"]).format).toBe("json");
    expect(run(["--jsonl"]).format).toBe("jsonl");
    expect(run(["--raw"]).format).toBe("raw");
  });

  test("reads model from env and rejects bad models", () => {
    expect(run([], { XAVIER_MODEL: "grok-4.6" }).model).toBe("grok-4.6");
    expect(() => run(["--model", "grok 4"] )).toThrow(UsageError);
  });

  test("rejects unknown flags", () => {
    expect(() => run(["--nope"])).toThrow(UsageError);
  });

  test("captures media and dry-run flags", () => {
    const options = run(["--images", "--videos", "--dry-run"]);
    expect(options.images).toBe(true);
    expect(options.videos).toBe(true);
    expect(options.dryRun).toBe(true);
  });
});
