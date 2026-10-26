import { describe, expect, test } from "bun:test";
import { estimateCost, formatUsd, roundUsd } from "../src/cost";

describe("estimateCost", () => {
  test("prices tokens and X Search items", () => {
    const cost = estimateCost({
      model: "grok-4.7",
      input: 1000,
      cached: 100,
      output: 200,
      xPosts: 3,
      xUsers: 0,
    });
    expect(cost).toBe(0.01805);
  });

  test("doubles token rates for long context", () => {
    const cost = estimateCost({
      model: "grok-4.7",
      input: 200_000,
      cached: 0,
      output: 0,
      xPosts: 0,
      xUsers: 0,
    });
    expect(cost).toBe(0.8);
  });

  test("returns null for unknown models", () => {
    expect(
      estimateCost({
        model: "mystery-model",
        input: 1,
        cached: 0,
        output: 1,
        xPosts: 0,
        xUsers: 0,
      }),
    ).toBeNull();
  });
});

describe("formatting", () => {
  test("rounds to six decimals", () => {
    expect(roundUsd(0.123456789)).toBe(0.123457);
  });

  test("formats USD", () => {
    expect(formatUsd(0.059564)).toBe("$0.0596");
    expect(formatUsd(null)).toBe("unknown");
  });
});
