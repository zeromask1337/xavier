import { describe, expect, test } from "bun:test";
import { addDays, isValidDateString, parseWhen } from "../src/dates";
import { UsageError } from "../src/errors";

const NOW = new Date("2026-10-25T12:00:00Z");

describe("isValidDateString", () => {
  test("accepts real dates", () => {
    expect(isValidDateString("2026-10-25")).toBe(true);
    expect(isValidDateString("2024-02-29")).toBe(true);
  });

  test("rejects invalid dates", () => {
    expect(isValidDateString("2026-02-30")).toBe(false);
    expect(isValidDateString("2025-02-29")).toBe(false);
    expect(isValidDateString("2026-13-01")).toBe(false);
    expect(isValidDateString("26-10-25")).toBe(false);
  });
});

describe("parseWhen", () => {
  test("passes through ISO dates", () => {
    expect(parseWhen("2026-10-01", NOW)).toBe("2026-10-01");
  });

  test("resolves named days relative to now", () => {
    expect(parseWhen("today", NOW)).toBe("2026-10-25");
    expect(parseWhen("yesterday", NOW)).toBe("2026-10-24");
    expect(parseWhen("tomorrow", NOW)).toBe("2026-10-26");
  });

  test("resolves relative windows", () => {
    expect(parseWhen("7d", NOW)).toBe("2026-10-18");
    expect(parseWhen("24h", NOW)).toBe("2026-10-24");
    expect(parseWhen("2w", NOW)).toBe("2026-10-11");
    expect(parseWhen("30m", NOW)).toBe("2026-10-25");
  });

  test("accepts ISO datetimes and keeps the written date", () => {
    expect(parseWhen("2026-10-24T23:30:00Z", NOW)).toBe("2026-10-24");
    expect(parseWhen("2026-10-24 08:15", NOW)).toBe("2026-10-24");
  });

  test("rejects nonsense", () => {
    expect(() => parseWhen("last tuesday", NOW)).toThrow(UsageError);
    expect(() => parseWhen("2026-02-30", NOW)).toThrow(UsageError);
    expect(() => parseWhen("", NOW)).toThrow(UsageError);
  });
});

describe("addDays", () => {
  test("crosses month and year boundaries", () => {
    expect(addDays("2026-10-31", 1)).toBe("2026-11-01");
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    expect(addDays("2026-01-01", -1)).toBe("2025-12-31");
    expect(addDays("2024-02-28", 1)).toBe("2024-02-29");
  });
});
