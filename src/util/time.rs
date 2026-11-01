//! Time helpers for commit metadata.

use chrono::{DateTime, FixedOffset, Local, Utc};

/// Current Unix timestamp plus the local UTC offset in minutes.
pub fn now() -> (i64, i32) {
    let now = Local::now();
    (now.timestamp(), now.offset().local_minus_utc() / 60)
}

/// Render minutes east of UTC as `+HHMM` / `-HHMM`.
pub fn format_offset(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let minutes = minutes.abs();
    format!("{sign}{:02}{:02}", minutes / 60, minutes % 60)
}

/// Parse `+HHMM` / `-HHMM` into minutes east of UTC.
pub fn parse_offset(input: &str) -> Option<i32> {
    let bytes = input.as_bytes();
    if bytes.len() != 5 || (bytes[0] != b'+' && bytes[0] != b'-') {
        return None;
    }
    let hours: i32 = input[1..3].parse().ok()?;
    let minutes: i32 = input[3..5].parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    let total = hours * 60 + minutes;
    Some(if bytes[0] == b'-' { -total } else { total })
}

/// Human readable local timestamp, e.g. `2026-05-29 12:00:00 +0800`.
pub fn format_timestamp(timestamp: i64, offset_minutes: i32) -> String {
    let Some(utc) = DateTime::<Utc>::from_timestamp(timestamp, 0) else {
        return format!("{timestamp}");
    };
    let Some(tz) = FixedOffset::east_opt(offset_minutes * 60) else {
        return utc.to_rfc3339();
    };
    utc.with_timezone(&tz)
        .format("%Y-%m-%d %H:%M:%S %z")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_roundtrip() {
        for minutes in [-720, -330, 0, 330, 480, 840] {
            let rendered = format_offset(minutes);
            assert_eq!(parse_offset(&rendered), Some(minutes));
        }
    }

    #[test]
    fn rejects_bad_offsets() {
        for input in ["", "+08", "+08000", "0800", "+ab00", "+2460"] {
            assert!(parse_offset(input).is_none(), "{input}");
        }
    }
}
