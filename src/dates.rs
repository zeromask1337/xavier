use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::errors::{AppError, Result};

fn parse_date(value: &str) -> Result<NaiveDate> {
    if !is_valid_date_string(value) {
        return Err(AppError::usage(format!("Invalid date \"{value}\".")));
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::usage(format!("Invalid date \"{value}\".")))
}

pub fn utc_today(now: &DateTime<Utc>) -> String {
    now.format("%Y-%m-%d").to_string()
}

pub fn is_valid_date_string(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    let Ok(year) = value[..4].parse::<i32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..].parse::<u32>() else {
        return false;
    };

    NaiveDate::from_ymd_opt(year, month, day).is_some()
}

/// Parse an absolute or relative date into a YYYY-MM-DD UTC string.
pub fn parse_when(input: &str, now: &DateTime<Utc>) -> Result<String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(AppError::usage("Date value is empty."));
    }

    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "today" => return Ok(utc_today(now)),
        "yesterday" => return add_days(&utc_today(now), -1),
        "tomorrow" => return add_days(&utc_today(now), 1),
        _ => {}
    }

    if let Some((amount, unit)) = parse_relative(&lower) {
        if amount > 36_500 {
            return Err(AppError::usage(format!(
                "Invalid date \"{input}\": relative amount is too large."
            )));
        }
        let unit_ms: i64 = match unit {
            'm' => 60_000,
            'h' => 3_600_000,
            'd' => 86_400_000,
            'w' => 7 * 86_400_000,
            _ => unreachable!("parse_relative only returns m/h/d/w"),
        };
        let millis = i64::try_from(amount)
            .ok()
            .and_then(|amount| amount.checked_mul(unit_ms))
            .ok_or_else(|| {
                AppError::usage(format!(
                    "Invalid date \"{input}\": relative amount is too large."
                ))
            })?;
        let shifted = *now - Duration::milliseconds(millis);
        return Ok(utc_today(&shifted));
    }

    if raw.len() == 10 && raw.as_bytes()[4] == b'-' {
        if !is_valid_date_string(raw) {
            return Err(AppError::usage(format!("Invalid date \"{input}\".")));
        }
        return Ok(raw.to_string());
    }

    if matches_iso_datetime(raw) {
        let date = &raw[..10];
        if is_valid_date_string(date) {
            return Ok(date.to_string());
        }
    }

    Err(AppError::usage(format!(
        "Invalid date \"{input}\". Use YYYY-MM-DD, a relative value like 7d or 24h, today, or yesterday."
    )))
}

/// Add days to a YYYY-MM-DD string, returning a YYYY-MM-DD string.
pub fn add_days(date: &str, days: i64) -> Result<String> {
    let parsed = parse_date(date)?;
    let duration = Duration::try_days(days)
        .ok_or_else(|| AppError::usage(format!("Invalid date \"{date}\".")))?;
    let shifted = parsed
        .checked_add_signed(duration)
        .ok_or_else(|| AppError::usage(format!("Invalid date \"{date}\".")))?;
    Ok(shifted.format("%Y-%m-%d").to_string())
}

/// Parse `^(\d+)\s*(m|h|d|w)$` after lower-casing.
fn parse_relative(value: &str) -> Option<(u128, char)> {
    let unit = value.chars().last()?;
    if !matches!(unit, 'm' | 'h' | 'd' | 'w') {
        return None;
    }
    let prefix = value[..value.len() - unit.len_utf8()].trim_end();
    if prefix.is_empty() || !prefix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let amount = prefix.parse::<u128>().ok()?;
    Some((amount, unit))
}

/// Match `^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?$`.
fn matches_iso_datetime(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 16 || !matches!(bytes[10], b'T' | b' ') {
        return false;
    }
    if !digits(bytes, 0, 4) || bytes[4] != b'-' || !digits(bytes, 5, 7) || bytes[7] != b'-' {
        return false;
    }
    if !digits(bytes, 8, 10)
        || !digits(bytes, 11, 13)
        || bytes[13] != b':'
        || !digits(bytes, 14, 16)
    {
        return false;
    }

    let mut index = 16;
    if index < bytes.len() && bytes[index] == b':' {
        if !digits(bytes, index + 1, index + 3) {
            return false;
        }
        index += 3;

        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index == start {
                return false;
            }
        }
    }

    if index == bytes.len() {
        return true;
    }

    if bytes[index] == b'Z' {
        return index + 1 == bytes.len();
    }

    if matches!(bytes[index], b'+' | b'-') {
        // [+-]HHMM
        if index + 5 == bytes.len()
            && digits(bytes, index + 1, index + 3)
            && digits(bytes, index + 3, index + 5)
        {
            return true;
        }
        // [+-]HH:MM
        if index + 6 == bytes.len()
            && digits(bytes, index + 1, index + 3)
            && bytes[index + 3] == b':'
            && digits(bytes, index + 4, index + 6)
        {
            return true;
        }
    }

    false
}

fn digits(bytes: &[u8], start: usize, end: usize) -> bool {
    end <= bytes.len() && start <= end && bytes[start..end].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()
    }

    #[test]
    fn accepts_real_dates() {
        assert!(is_valid_date_string("2026-10-25"));
        assert!(is_valid_date_string("2024-02-29"));
    }

    #[test]
    fn rejects_invalid_dates() {
        assert!(!is_valid_date_string("2026-02-30"));
        assert!(!is_valid_date_string("2025-02-29"));
        assert!(!is_valid_date_string("2026-13-01"));
        assert!(!is_valid_date_string("26-10-25"));
    }

    #[test]
    fn passes_through_iso_dates() {
        assert_eq!(parse_when("2026-10-01", &now()).unwrap(), "2026-10-01");
    }

    #[test]
    fn resolves_named_days() {
        assert_eq!(parse_when("today", &now()).unwrap(), "2026-10-25");
        assert_eq!(parse_when("yesterday", &now()).unwrap(), "2026-10-24");
        assert_eq!(parse_when("tomorrow", &now()).unwrap(), "2026-10-26");
    }

    #[test]
    fn resolves_relative_windows() {
        assert_eq!(parse_when("7d", &now()).unwrap(), "2026-10-18");
        assert_eq!(parse_when("24h", &now()).unwrap(), "2026-10-24");
        assert_eq!(parse_when("2w", &now()).unwrap(), "2026-10-11");
        assert_eq!(parse_when("30m", &now()).unwrap(), "2026-10-25");
    }

    #[test]
    fn accepts_iso_datetimes_and_keeps_the_written_date() {
        assert_eq!(
            parse_when("2026-10-24T23:30:00Z", &now()).unwrap(),
            "2026-10-24"
        );
        assert_eq!(
            parse_when("2026-10-24 08:15", &now()).unwrap(),
            "2026-10-24"
        );
        assert_eq!(
            parse_when("2026-10-24T08:15:30.123+02:00", &now()).unwrap(),
            "2026-10-24"
        );
    }

    #[test]
    fn rejects_nonsense() {
        assert!(parse_when("last tuesday", &now()).is_err());
        assert!(parse_when("2026-02-30", &now()).is_err());
        assert!(parse_when("", &now()).is_err());
    }

    #[test]
    fn adds_days_across_boundaries() {
        assert_eq!(add_days("2026-10-31", 1).unwrap(), "2026-11-01");
        assert_eq!(add_days("2026-12-31", 1).unwrap(), "2027-01-01");
        assert_eq!(add_days("2026-01-01", -1).unwrap(), "2025-12-31");
        assert_eq!(add_days("2024-02-28", 1).unwrap(), "2024-02-29");
    }
}
