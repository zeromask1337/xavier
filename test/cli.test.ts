import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import fixture from "./fixtures/response.json";

let server: ReturnType<typeof Bun.serve>;
let baseUrl: string;
let requestCount = 0;

beforeAll(() => {
  server = Bun.serve({
    port: 0,
    fetch(req) {
      const url = new URL(req.url);
      if (url.pathname === "/v1/responses") {
        requestCount += 1;
        return new Response(JSON.stringify(fixture), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.pathname === "/v1/stub/responses") {
        const stub = structuredClone(fixture) as any;
        stub.output[2].content[0].text = '{"summary":"placeholder","posts":[]}';
        return new Response(JSON.stringify(stub), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response("not found", { status: 404 });
    },
  });
  baseUrl = `http://127.0.0.1:${server.port}/v1`;
});

afterAll(() => {
  server.stop(true);
});

interface RunResult {
  stdout: string;
  stderr: string;
  code: number;
}

async function runCli(
  args: string[],
  env: Record<string, string> = {},
): Promise<RunResult> {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      XAI_API_KEY: "test-key",
      XAI_BASE_URL: baseUrl,
      ...env,
    } as Record<string, string>,
    stdout: "pipe",
    stderr: "pipe",
  });

  const [stdout, stderr, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { stdout, stderr, code };
}

describe("cli", () => {
  test("renders agent-first Markdown by default", async () => {
    const result = await runCli(["grok 4.7", "--limit", "5"]);
    expect(result.code).toBe(0);
    expect(result.stderr).toBe("");
    expect(result.stdout).toContain("# X search: grok 4.7");
    expect(result.stdout).toContain("model=grok-4.7");
    expect(result.stdout).toContain("## Posts");
    expect(result.stdout).toContain("https://x.com/alice/status/111");
  });

  test("emits structured JSON", async () => {
    const result = await runCli(["grok 4.7", "--json"]);
    expect(result.code).toBe(0);
    const parsed = JSON.parse(result.stdout);
    expect(parsed.posts).toHaveLength(2);
    expect(parsed.meta.usage.cost_usd).toBe(0.001234);
    expect(parsed.meta.warnings).toHaveLength(1);
  });

  test("emits JSONL", async () => {
    const result = await runCli(["grok 4.7", "--jsonl"]);
    const lines = result.stdout.trimEnd().split("\n");
    expect(lines).toHaveLength(3);
    expect(JSON.parse(lines[0] ?? "").type).toBe("result");
    expect(JSON.parse(lines[1] ?? "").type).toBe("post");
  });

  test("passes raw payloads through without validation", async () => {
    const result = await runCli(["grok 4.7", "--raw"]);
    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout).id).toBe("resp_test_123");
  });

  test("prints the request body on --dry-run and does not call the API", async () => {
    const before = requestCount;
    const result = await runCli(["--dry-run", "grok 4.7", "--from", "7d"]);
    expect(result.code).toBe(0);
    expect(requestCount).toBe(before);
    const body = JSON.parse(result.stdout);
    expect(body.model).toBe("grok-4.7");
    expect(body.tools[0].type).toBe("x_search");
  });

  test("fails with exit code 2 when the key is missing", async () => {
    const result = await runCli(["grok 4.7"], { XAI_API_KEY: "" });
    expect(result.code).toBe(2);
    expect(result.stderr).toContain("XAI_API_KEY is not set");
  });

  test("fails with exit code 2 on usage errors", async () => {
    const result = await runCli(["--limit", "0", "grok 4.7"]);
    expect(result.code).toBe(2);
    expect(result.stderr).toContain("--limit");
  });

  test("fails with exit code 1 on API errors", async () => {
    const result = await runCli(["grok 4.7"], {
      XAI_BASE_URL: `${baseUrl}/missing`,
    });
    expect(result.code).toBe(1);
    expect(result.stderr).toContain("xAI API error 404");
  });

  test("rejects placeholder stubs unless --raw is used", async () => {
    const stubBase = { XAI_BASE_URL: `${baseUrl}/stub` };

    const normal = await runCli(["grok 4.7"], stubBase);
    expect(normal.code).toBe(1);
    expect(normal.stderr).toContain("placeholder result");

    const raw = await runCli(["grok 4.7", "--raw"], stubBase);
    expect(raw.code).toBe(0);
    expect(JSON.parse(raw.stdout).id).toBe("resp_test_123");
  });

  test("reads the query from stdin", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "-", "--dry-run"], {
      cwd: process.cwd(),
      env: process.env as Record<string, string>,
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    proc.stdin.write("query from stdin\n");
    await proc.stdin.end();

    const stdout = await new Response(proc.stdout).text();
    await proc.exited;
    expect(JSON.parse(stdout).input[1].content).toContain("Search X for: query from stdin");
  });
});
