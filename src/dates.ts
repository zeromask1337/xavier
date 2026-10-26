import { UsageError } from "./errors";

const DAY_MS = 86_400_000;

export function utcToday(now: Date = new Date()): string {
  return now.toISOString().slice(0, 10);
}

export function isValidDateString(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const parts = value.split("-");
  const y = Number(parts[0] ?? "");
  const m = Number(parts[1] ?? "");
  const d = Number(parts[2] ?? "");
  if (!Number.isInteger(y) || !Number.isInteger(m) || !Number.isInteger(d)) return false;
  const dt = new Date(Date.UTC(y, m - 1, d));
  return dt.getUTCFullYear() === y && dt.getUTCMonth() === m - 1 && dt.getUTCDate() === d;
}

/** Parse an absolute or relative date into a YYYY-MM-DD UTC string. */
export function parseWhen(input: string, now: Date = new Date()): string {
  const raw = input.trim();
  if (!raw) throw new UsageError("Date value is empty.");

  const lower = raw.toLowerCase();
  if (lower === "today") return utcToday(now);
  if (lower === "yesterday") return addDays(utcToday(now), -1);
  if (lower === "tomorrow") return addDays(utcToday(now), 1);

  const relative = /^(\d+)\s*(m|h|d|w)$/.exec(lower);
  if (relative) {
    const amount = Number(relative[1]);
    const unit = relative[2];
    if (!Number.isSafeInteger(amount) || amount > 36_500) {
      throw new UsageError(`Invalid date "${input}": relative amount is too large.`);
    }
    const unitMs =
      unit === "m" ? 60_000 : unit === "h" ? 3_600_000 : unit === "d" ? DAY_MS : 7 * DAY_MS;
    return utcToday(new Date(now.getTime() - amount * unitMs));
  }

  if (/^\d{4}-\d{2}-\d{2}$/.test(raw)) {
    if (!isValidDateString(raw)) throw new UsageError(`Invalid date "${input}".`);
    return raw;
  }

  const iso = raw.match(
    /^(\d{4}-\d{2}-\d{2})[T ]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?$/,
  );
  if (iso?.[1] && isValidDateString(iso[1])) return iso[1];

  throw new UsageError(
    `Invalid date "${input}". Use YYYY-MM-DD, a relative value like 7d or 24h, today, or yesterday.`,
  );
}

/** Add days to a YYYY-MM-DD string, returning a YYYY-MM-DD string. */
export function addDays(date: string, days: number): string {
  if (!isValidDateString(date)) throw new UsageError(`Invalid date "${date}".`);
  const parts = date.split("-");
  const y = Number(parts[0]);
  const m = Number(parts[1]);
  const d = Number(parts[2]);
  return new Date(Date.UTC(y, m - 1, d + days)).toISOString().slice(0, 10);
}
