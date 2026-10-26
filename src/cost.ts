/**
 * Fallback cost estimate. The xAI Responses API usually returns an exact
 * `usage.cost_in_usd_ticks` value, which is always preferred.
 */
const PRICES: Record<string, { input: number; cached: number; output: number }> = {
  "grok-4.7": { input: 2, cached: 0.5, output: 6 },
  "grok-4.6": { input: 2, cached: 0.5, output: 6 },
  "grok-4.5": { input: 2, cached: 0.3, output: 6 },
  "grok-4.3": { input: 1.25, cached: 0.2, output: 2.5 },
  "grok-4.20-0309-reasoning": { input: 1.25, cached: 0.2, output: 2.5 },
  "grok-4.20-0309-non-reasoning": { input: 1.25, cached: 0.2, output: 2.5 },
  "grok-4.20-multi-agent-0309": { input: 1.25, cached: 0.2, output: 2.5 },
  "grok-build-0.1": { input: 1, cached: 0.2, output: 2 },
};

const X_POST_USD = 0.005;
const X_USER_USD = 0.01;

export interface CostInput {
  model: string;
  input: number;
  cached: number;
  output: number;
  xPosts: number;
  xUsers: number;
}

export function estimateCost(i: CostInput): number | null {
  const price = PRICES[i.model];
  if (!price) return null;

  const longContext = i.input >= 200_000;
  const multiplier = longContext ? 2 : 1;
  const uncached = Math.max(0, i.input - i.cached);
  const tokenCost =
    ((uncached * price.input + i.cached * price.cached + i.output * price.output) / 1_000_000) *
    multiplier;
  const toolCost = i.xPosts * X_POST_USD + i.xUsers * X_USER_USD;
  return roundUsd(tokenCost + toolCost);
}

export function roundUsd(value: number): number {
  return Math.round(value * 1_000_000) / 1_000_000;
}

export function formatUsd(value: number | null): string {
  if (value == null) return "unknown";
  return `$${value.toFixed(4)}`;
}
