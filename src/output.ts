import type { SearchResult } from "./types";

export function renderMarkdown(result: SearchResult): string {
  const { query, summary, posts, meta } = result;
  const lines: string[] = [];

  lines.push(`# X search: ${oneLine(query)}`, "");

  const stats = [
    `model=${meta.model}`,
    `effort=${meta.effort}`,
    `posts=${posts.length}`,
    `fetched=${meta.usage.x_posts_fetched}`,
    `searches=${meta.usage.x_search_calls}`,
    `cost_usd=${meta.usage.cost_usd == null ? "unknown" : meta.usage.cost_usd.toFixed(4)}`,
    `cost_source=${meta.usage.cost_source ?? "unknown"}`,
    `elapsed_ms=${meta.elapsed_ms}`,
    `limit=${meta.filters.limit}`,
  ].join(" ");
  lines.push(stats);

  const filters: string[] = [];
  if (meta.filters.from) filters.push(`from=${meta.filters.from}`);
  if (meta.filters.to) filters.push(`to=${meta.filters.to}`);
  if (meta.filters.handles.length > 0) {
    filters.push(`handles=${meta.filters.handles.map((h) => `@${h}`).join(",")}`);
  }
  if (meta.filters.exclude_handles.length > 0) {
    filters.push(`exclude=${meta.filters.exclude_handles.map((h) => `@${h}`).join(",")}`);
  }
  if (meta.filters.sort) filters.push(`sort=${meta.filters.sort}`);
  if (filters.length > 0) lines.push(filters.join(" "));

  if (meta.warnings.length > 0) {
    lines.push("", "## Warnings");
    for (const warning of meta.warnings) lines.push(`- ${warning}`);
  }

  lines.push("", "## Summary", summary || "No summary returned.");
  lines.push("", "## Posts");

  if (posts.length === 0) {
    lines.push("No matching posts.");
  } else {
    posts.forEach((post, index) => {
      const parts = [`${index + 1}. @${post.author}`];
      if (post.date) parts.push(post.date);
      parts.push(post.url);
      if (!post.verified) parts.push("unverified");
      lines.push(parts.join(" | "));
      if (post.excerpt) lines.push(`   > ${post.excerpt}`);
    });
  }

  return `${lines.join("\n")}\n`;
}

export function renderJson(result: SearchResult): string {
  return `${JSON.stringify(result, null, 2)}\n`;
}

export function renderJsonl(result: SearchResult): string {
  const { posts, ...rest } = result;
  const lines = [JSON.stringify({ type: "result", ...rest })];
  for (const post of posts) lines.push(JSON.stringify({ type: "post", ...post }));
  return `${lines.join("\n")}\n`;
}

function oneLine(value: string): string {
  return value.replace(/\s+/g, " ").trim();
}
