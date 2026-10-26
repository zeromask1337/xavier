export interface Post {
  /** Canonical x.com URL. */
  url: string;
  /** Status id used for de-duplication and citation checks. */
  id: string;
  /** Author handle without the leading @. */
  author: string;
  /** YYYY-MM-DD in UTC, or null when unknown. */
  date: string | null;
  /** Verbatim excerpt, whitespace-collapsed. */
  excerpt: string;
  /** True when the post URL appeared in the X Search citations. */
  verified: boolean;
}

export interface SearchCall {
  name: string;
  query: string | null;
  mode: string | null;
}

export interface Usage {
  input_tokens: number;
  cached_input_tokens: number;
  output_tokens: number;
  reasoning_tokens: number;
  total_tokens: number;
  x_search_calls: number;
  x_posts_fetched: number;
  x_users_fetched: number;
  cost_usd: number | null;
  cost_source: "api" | "estimated" | null;
}

export interface Filters {
  from: string | null;
  to: string | null;
  handles: string[];
  exclude_handles: string[];
  limit: number;
  sort: string | null;
}

export interface Meta {
  model: string;
  response_id: string | null;
  created_at: string;
  elapsed_ms: number;
  effort: string;
  max_turns: number;
  filters: Filters;
  usage: Usage;
  searches: SearchCall[];
  citations: number;
  warnings: string[];
}

export interface SearchResult {
  query: string;
  summary: string;
  posts: Post[];
  meta: Meta;
}
