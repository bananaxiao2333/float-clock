//! Time parsing and T± formatting.
//!
//! T± semantics (uniform across the whole program):
//!
//! ```text
//! T-00:05:00   5 minutes 00 seconds remain until that moment
//! T+00:00:12   that moment passed 12 seconds ago
//! T-00:00:00   on the dot (the very second that contains that moment)
//! ```
//!
//! The program works with two moments:
//!
//! * target moment `T`               - subtitle (how long ago the target passed)
//! * offset moment `M = T + offset`  - main title (how long until the offset)
//!
//! All times are computed in the machine's local (naive) time.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};

/// Parse a duration and return it in seconds. Supports `30` / `-30` / `90s` / `5m`
/// / `1h30m` / `1d2h` / `00:05:00` / `5:00` / `-00:05:00`.
pub fn parse_duration(spec: &str) -> Result<f64, String> {
    let text = spec.trim();
    if text.is_empty() {
        return Ok(0.0);
    }
    if let Some(value) = parse_plain_number(text) {
        return Ok(value);
    }
    if text.contains(':') {
        return parse_colon(text);
    }
    parse_units(text)
}

fn parse_plain_number(text: &str) -> Option<f64> {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if body.is_empty() {
        return None;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    for ch in body.chars() {
        if ch.is_ascii_digit() {
            seen_digit = true;
        } else if ch == '.' && !seen_dot {
            seen_dot = true;
        } else {
            return None;
        }
    }
    if !seen_digit {
        return None;
    }
    text.parse::<f64>().ok()
}

/// `HH:MM:SS` / `MM:SS` / `DD:HH:MM:SS`, with an optional leading sign.
fn parse_colon(text: &str) -> Result<f64, String> {
    let (sign, body) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text.strip_prefix('+').unwrap_or(text)),
    };
    let parts: Vec<&str> = body.split(':').collect();
    if parts.is_empty() || parts.len() > 4 {
        return Err(format!("unrecognized duration: {text:?}"));
    }
    let mut values = [0i64; 4]; // seconds, minutes, hours, days
    for (index, part) in parts.iter().rev().enumerate() {
        let part = part.trim();
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("unrecognized duration: {text:?}"));
        }
        values[index] = part
            .parse::<i64>()
            .map_err(|_| format!("unrecognized duration: {text:?}"))?;
    }
    let [sec, minute, hour, day] = values;
    Ok(sign * (day * 86400 + hour * 3600 + minute * 60 + sec) as f64)
}

/// The `1d2h3m4s` unit form (units may be omitted, but at least one is required).
fn parse_units(text: &str) -> Result<f64, String> {
    let bad = || format!("unrecognized duration: {text:?} (examples: -00:05:00 / 1h30m / 90)");
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };

    let mut total = 0.0f64;
    let mut rest = body.trim();
    let mut found = false;

    while !rest.is_empty() {
        let digits_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        if digits_end == 0 {
            return Err(bad());
        }
        let number: f64 = rest[..digits_end].parse().map_err(|_| bad())?;
        let mut tail = rest[digits_end..].trim_start();

        // allow the spaced spelling "1 h 30 m"
        let unit_end = tail
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(tail.len());
        let unit = tail[..unit_end].to_ascii_lowercase();
        tail = &tail[unit_end..];

        let scale = match unit.as_str() {
            "d" | "day" | "days" => 86400.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            _ => return Err(bad()),
        };
        total += number * scale;
        found = true;
        rest = tail.trim_start();
    }

    if !found {
        return Err(bad());
    }
    Ok(if negative { -total } else { total })
}

/// Parse the target moment.
///
/// * `2026-01-01T09:30:00` / `2026-01-01 09:30` / `2026-01-01` (ISO and common spellings)
/// * `09:30` / `09:30:00` (that clock time today; rolls over to tomorrow once it has passed)
/// * `+1h30m` / `-10m` (relative to now, unit required)
pub fn parse_target(spec: &str, now: NaiveDateTime) -> Result<NaiveDateTime, String> {
    let text = spec.trim();
    if text.is_empty() {
        return Err("target moment is empty; set it in [time] target of config.toml".into());
    }

    // relative to now: +1h30m / -10m
    if text.starts_with(['+', '-']) && text.chars().last().is_some_and(|c| c.is_ascii_alphabetic())
    {
        if let Ok(delta) = parse_duration(text) {
            if delta != 0.0 {
                return Ok(now + Duration::milliseconds((delta * 1000.0).round() as i64));
            }
        }
    }

    // time only: that clock time today, tomorrow once it has passed
    if let Some((hour, minute, second)) = parse_time_only(text) {
        if hour > 23 || minute > 59 || second > 59 {
            return Err(format!("time out of range: {spec:?}"));
        }
        let target = now
            .date()
            .and_hms_opt(hour, minute, second)
            .ok_or_else(|| format!("time out of range: {spec:?}"))?;
        return Ok(if target <= now {
            target + Duration::days(1)
        } else {
            target
        });
    }

    // Normalize the date separators to '-' and the date/time separator to 'T',
    // so only three formats have to be recognized; the missing-year and
    // missing-year-month cases are handled by prepending prefixes.
    let normalized = normalize_datetime(text);
    const DATETIME_FORMATS: &[&str] = &[
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ];
    const DATE_FORMATS: &[&str] = &["%Y-%m-%d"];

    let candidates = [
        normalized.clone(),
        format!("{}-{normalized}", now.year()),
        format!("{}-{:02}-{normalized}", now.year(), now.month()),
    ];
    for candidate in &candidates {
        for format in DATETIME_FORMATS {
            if let Ok(parsed) = NaiveDateTime::parse_from_str(candidate, format) {
                return Ok(parsed);
            }
        }
    }
    for candidate in &candidates {
        for format in DATE_FORMATS {
            if let Ok(date) = NaiveDate::parse_from_str(candidate, format) {
                return Ok(date.and_hms_opt(0, 0, 0).unwrap());
            }
        }
    }
    Err(format!(
        "unrecognized time format: {spec:?} (examples: 2026-01-01T09:30:00 / 2026-01-01 09:30 / 09:30 / +1h30m)"
    ))
}

/// Unify the date separators and turn the space between date and time into `T`.
fn normalize_datetime(text: &str) -> String {
    let (date_part, time_part) = match text.find(['T', 't', ' ']) {
        Some(index) => (&text[..index], Some(&text[index + 1..])),
        None => (text, None),
    };
    let date_part: String = date_part
        .chars()
        .map(|ch| if ch == '/' || ch == '.' { '-' } else { ch })
        .collect();
    match time_part {
        Some(time) if !time.trim().is_empty() => format!("{date_part}T{}", time.trim()),
        _ => date_part,
    }
}

fn parse_time_only(text: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let mut numbers = [0u32; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || part.len() > 2 || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        numbers[index] = part.parse().ok()?;
    }
    Some((numbers[0], numbers[1], numbers[2]))
}

/// Split the "moment - now" difference in seconds into (sign, whole seconds for zero padding).
///
/// Not reached yet gives `('-', remaining seconds)`; already passed gives
/// `('+', elapsed seconds)`.
/// The remaining amount is rounded up and the elapsed amount rounded down, so the
/// exact second of the moment shows `T-00:00:00`.
pub fn split_delta(delta_seconds: f64) -> (char, i64) {
    if delta_seconds >= 0.0 {
        ('-', delta_seconds.ceil() as i64)
    } else {
        ('+', (-delta_seconds).floor() as i64)
    }
}

/// Zero-padded fixed-width time string: `HH:MM:SS`, or `DD:HH:MM:SS` beyond a day.
pub fn format_hms(total_seconds: i64, show_days: bool) -> String {
    let seconds = total_seconds.max(0);
    let days = seconds / 86400;
    let rest = seconds % 86400;
    let hours = rest / 3600;
    let minutes = (rest % 3600) / 60;
    let secs = rest % 60;
    if days > 0 && show_days {
        format!("{days:02}:{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{:02}:{minutes:02}:{secs:02}", days * 24 + hours)
    }
}

/// Compact human-readable duration for notification bodies: `1h 30m`.
///
/// Zero-valued components are omitted and components are joined by one space.
/// Seconds are only ever shown while the duration is under a day, so the output
/// stays short for long spans (`1d 1h 1m`, never `1d 1h 1m 1s`); an all-zero
/// duration falls back to `0s` rather than an empty string. Negative input is
/// clamped to zero — the caller renders the sign itself.
pub fn format_human(total_seconds: f64) -> String {
    let seconds = total_seconds.round().max(0.0) as i64;
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if secs > 0 && days == 0 {
        parts.push(format!("{secs}s"));
    }
    if parts.is_empty() {
        "0s".into()
    } else {
        parts.join(" ")
    }
}

const INFO_TOKENS: [&str; 7] = [
    "datetime",
    "mark_datetime",
    "date",
    "time",
    "mark",
    "delta_human",
    "delta",
];

/// Render the "target moment + offset" line.
///
/// Available placeholders: `{date} {time} {datetime}` (target moment T),
/// `{mark} {mark_datetime}` (offset moment M), `{delta}` (+02:00:00),
/// `{delta_human}` (+2h). Placeholders that are not recognized are kept verbatim.
pub fn render_info(
    template: &str,
    target: NaiveDateTime,
    mark: NaiveDateTime,
    offset_seconds: f64,
    show_days: bool,
) -> String {
    if template.trim().is_empty() {
        return String::new();
    }
    let sign = if offset_seconds >= 0.0 { "+" } else { "-" };
    let magnitude = offset_seconds.abs().round() as i64;
    let values = [
        ("datetime", target.format("%Y-%m-%d %H:%M:%S").to_string()),
        (
            "mark_datetime",
            mark.format("%Y-%m-%d %H:%M:%S").to_string(),
        ),
        ("date", target.format("%Y-%m-%d").to_string()),
        ("time", target.format("%H:%M:%S").to_string()),
        ("mark", mark.format("%H:%M:%S").to_string()),
        (
            "delta_human",
            format!("{sign}{}", format_human(magnitude as f64)),
        ),
        (
            "delta",
            format!("{sign}{}", format_hms(magnitude, show_days)),
        ),
    ];
    let mut text = template.to_string();
    for key in INFO_TOKENS {
        let value = values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.as_str())
            .unwrap_or("");
        text = text.replace(&format!("{{{key}}}"), value);
    }
    text
}

/// The target moment's HH:MM:SS (used in notifications and diagnostics).
pub fn clock_of(moment: NaiveDateTime) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        moment.hour(),
        moment.minute(),
        moment.second()
    )
}

/// The target moment's YYYY-MM-DD HH:MM:SS.
pub fn datetime_of(moment: NaiveDateTime) -> String {
    moment.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    #[test]
    fn duration_plain_numbers() {
        assert_eq!(parse_duration("30").unwrap(), 30.0);
        assert_eq!(parse_duration("-30").unwrap(), -30.0);
        assert_eq!(parse_duration("1.5").unwrap(), 1.5);
        assert_eq!(parse_duration("").unwrap(), 0.0);
        assert_eq!(parse_duration("  ").unwrap(), 0.0);
    }

    #[test]
    fn duration_colon_forms() {
        assert_eq!(parse_duration("00:05:00").unwrap(), 300.0);
        assert_eq!(parse_duration("-00:05:00").unwrap(), -300.0);
        assert_eq!(parse_duration("5:00").unwrap(), 300.0);
        assert_eq!(parse_duration("1:00:00:00").unwrap(), 86400.0);
        assert_eq!(parse_duration("+00:00:30").unwrap(), 30.0);
    }

    #[test]
    fn duration_unit_forms() {
        assert_eq!(parse_duration("90s").unwrap(), 90.0);
        assert_eq!(parse_duration("5m").unwrap(), 300.0);
        assert_eq!(parse_duration("1h30m").unwrap(), 5400.0);
        assert_eq!(parse_duration("1d2h").unwrap(), 93600.0);
        assert_eq!(parse_duration("-10m").unwrap(), -600.0);
        assert_eq!(parse_duration("2 hours").unwrap(), 7200.0);
        assert_eq!(parse_duration("120min").unwrap(), 7200.0);
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("12x").is_err());
    }

    #[test]
    fn target_time_only_rolls_to_tomorrow() {
        let now = dt(2026, 9, 19, 21, 0, 0);
        assert_eq!(
            parse_target("22:00", now).unwrap(),
            dt(2026, 9, 19, 22, 0, 0)
        );
        assert_eq!(
            parse_target("20:29", now).unwrap(),
            dt(2026, 9, 20, 20, 29, 0)
        );
        assert_eq!(
            parse_target("22:30:15", now).unwrap(),
            dt(2026, 9, 19, 22, 30, 15)
        );
        assert!(parse_target("25:00", now).is_err());
    }

    #[test]
    fn target_absolute_forms() {
        let now = dt(2026, 9, 19, 21, 0, 0);
        assert_eq!(
            parse_target("2026-09-19T20:29:00", now).unwrap(),
            dt(2026, 9, 19, 20, 29, 0)
        );
        assert_eq!(
            parse_target("2026-09-19 20:29", now).unwrap(),
            dt(2026, 9, 19, 20, 29, 0)
        );
        assert_eq!(
            parse_target("2026/09/19 20:29", now).unwrap(),
            dt(2026, 9, 19, 20, 29, 0)
        );
        assert_eq!(
            parse_target("2026-09-19", now).unwrap(),
            dt(2026, 9, 19, 0, 0, 0)
        );
        assert_eq!(
            parse_target("09-19 20:29", now).unwrap(),
            dt(2026, 9, 19, 20, 29, 0)
        );
        assert!(parse_target("", now).is_err());
        assert!(parse_target("nonsense", now).is_err());
    }

    #[test]
    fn target_relative_forms() {
        let now = dt(2026, 9, 19, 21, 0, 0);
        assert_eq!(
            parse_target("+1h30m", now).unwrap(),
            dt(2026, 9, 19, 22, 30, 0)
        );
        assert_eq!(
            parse_target("-10m", now).unwrap(),
            dt(2026, 9, 19, 20, 50, 0)
        );
    }

    #[test]
    fn split_delta_rounds_the_right_way() {
        assert_eq!(split_delta(0.0), ('-', 0));
        assert_eq!(split_delta(0.4), ('-', 1));
        assert_eq!(split_delta(-0.4), ('+', 0));
        assert_eq!(split_delta(-12.9), ('+', 12));
        assert_eq!(split_delta(12.9), ('-', 13));
    }

    #[test]
    fn hms_pads_every_field() {
        assert_eq!(format_hms(0, true), "00:00:00");
        assert_eq!(format_hms(59, true), "00:00:59");
        assert_eq!(format_hms(3661, true), "01:01:01");
        assert_eq!(format_hms(90000, true), "01:01:00:00");
        assert_eq!(format_hms(90000, false), "25:00:00");
        assert_eq!(format_hms(-5, true), "00:00:00");
    }

    #[test]
    fn human_duration_is_compact() {
        assert_eq!(format_human(0.0), "0s");
        assert_eq!(format_human(45.0), "45s");
        assert_eq!(format_human(120.0), "2m");
        assert_eq!(format_human(5400.0), "1h 30m");
        assert_eq!(format_human(90061.0), "1d 1h 1m");
        // under a day the seconds are kept, beyond a day they are dropped
        assert_eq!(format_human(3725.0), "1h 2m 5s");
        // the caller renders the sign, so the magnitude is clamped to zero here
        assert_eq!(format_human(-5.0), "0s");
    }

    #[test]
    fn info_line_renders_tokens() {
        let target = dt(2026, 9, 19, 20, 29, 0);
        let mark = dt(2026, 9, 19, 22, 29, 0);
        assert_eq!(
            render_info("{time} | {delta}", target, mark, 7200.0, true),
            "20:29:00 | +02:00:00"
        );
        assert_eq!(
            render_info("{delta_human}", target, mark, -300.0, true),
            "-5m"
        );
        assert_eq!(
            render_info("{date} {mark}", target, mark, 7200.0, true),
            "2026-09-19 22:29:00"
        );
        // unknown placeholders are kept verbatim
        assert_eq!(
            render_info("{nope} {time}", target, mark, 0.0, true),
            "{nope} 20:29:00"
        );
        assert_eq!(render_info("   ", target, mark, 0.0, true), "");
    }
}
