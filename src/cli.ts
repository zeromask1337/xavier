#!/usr/bin/env bun
import pkg from "../package.json";
import { helpText, parseCli } from "./args";
import { formatUsd } from "./cost";
import { UsageError, XavierError } from "./errors";
import { renderJson, renderJsonl, renderMarkdown } from "./output";
import { buildRequestBody, buildResult, requestXPosts } from "./xai";

async function main(argv: string[]): Promise<number> {
  let parsed;
  try {
    parsed = parseCli(argv);
  } catch (err) {
    if (err instanceof UsageError) {
      process.stderr.write(`xavier: ${err.message}\n\n${helpText()}`);
      return err.exitCode;
    }
    throw err;
  }

  if (parsed.kind === "help") {
    process.stdout.write(helpText());
    return 0;
  }
  if (parsed.kind === "version") {
    process.stdout.write(`xavier ${pkg.version}\n`);
    return 0;
  }

  const options = parsed.options;
  let query = options.query;

  if (query === "-" || (query === "" && !process.stdin.isTTY)) {
    const piped = await readStdin();
    if (query === "-") query = piped;
    else if (piped !== "") query = piped;
  }

  query = query.replace(/\s+/g, " ").trim();
  if (query === "") {
    process.stderr.write(
      "xavier: missing query.\n\nUsage: xavier [search] <query> [options]\n       echo \"<query>\" | xavier -\n",
    );
    return 2;
  }
  options.query = query;

  if (options.dryRun) {
    process.stdout.write(`${JSON.stringify(buildRequestBody(options), null, 2)}\n`);
    return 0;
  }

  const apiKey = process.env.XAI_API_KEY?.trim();
  if (!apiKey) {
    process.stderr.write(
      "xavier: XAI_API_KEY is not set.\n" +
        "  Run with: doppler run -- xavier \"your query\"\n" +
        "  Or: export XAI_API_KEY=...\n",
    );
    return 2;
  }

  const { raw, elapsedMs } = await requestXPosts(options, apiKey);

  if (options.format === "raw") {
    process.stdout.write(`${JSON.stringify(raw, null, 2)}\n`);
    if (options.verbose) {
      process.stderr.write(`xavier: raw response received in ${elapsedMs}ms\n`);
    }
    return 0;
  }

  const result = buildResult(raw, options, elapsedMs);

  switch (options.format) {
    case "json":
      process.stdout.write(renderJson(result));
      break;
    case "jsonl":
      process.stdout.write(renderJsonl(result));
      break;
    case "md":
      process.stdout.write(renderMarkdown(result));
      break;
  }

  if (options.verbose) {
    const diagnostics = [
      `model=${options.model} effort=${options.effort} max_turns=${options.maxTurns}`,
      ...result.meta.searches.map(
        (search) =>
          `search ${search.name}${search.query ? ` "${search.query}"` : ""}${
            search.mode ? ` (${search.mode})` : ""
          }`,
      ),
      `response=${result.meta.response_id ?? "unknown"} posts=${result.posts.length} fetched=${
        result.meta.usage.x_posts_fetched
      } cost=${formatUsd(result.meta.usage.cost_usd)} elapsed=${result.meta.elapsed_ms}ms`,
    ];
    process.stderr.write(diagnostics.map((line) => `xavier: ${line}`).join("\n") + "\n");
  }

  return 0;
}

async function readStdin(): Promise<string> {
  const chunks: Uint8Array[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : (chunk as Uint8Array));
  }
  return Buffer.concat(chunks).toString("utf8").trim();
}

main(process.argv.slice(2))
  .then((code) => {
    process.exitCode = code;
  })
  .catch((err) => {
    if (err instanceof UsageError) {
      process.stderr.write(`xavier: ${err.message}\n`);
      process.exitCode = err.exitCode;
      return;
    }
    if (err instanceof XavierError) {
      process.stderr.write(`xavier: ${err.message}\n`);
      if (err.hint) process.stderr.write(`  hint: ${err.hint}\n`);
      process.exitCode = err.exitCode;
      return;
    }
    const message = err instanceof Error ? (err.stack ?? err.message) : String(err);
    process.stderr.write(`xavier: unexpected error: ${message}\n`);
    process.exitCode = 1;
  });
