use jiff::{
    civil::{DateTime, Time},
    fmt::{rfc2822, temporal::DateTimePrinter},
    tz::{AmbiguousOffset, Offset, TimeZone},
    SignedDuration, Timestamp, Zoned,
};

const RFC3339_PRINTER: DateTimePrinter = DateTimePrinter::new();
const RFC3339_MILLIS_PRINTER: DateTimePrinter = DateTimePrinter::new().precision(Some(3));
#[allow(dead_code)]
const RFC2822_PRINTER: rfc2822::DateTimePrinter = rfc2822::DateTimePrinter::new();
const RFC2822_PARSER: rfc2822::DateTimeParser = rfc2822::DateTimeParser::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CivilTimestamp {
    Unambiguous(Timestamp),
    Fold(Timestamp),
    Gap,
}

pub fn now() -> Timestamp {
    Timestamp::now()
}

pub fn parse_rfc3339(input: &str) -> Result<Timestamp, jiff::Error> {
    input.parse()
}

#[allow(dead_code)]
pub fn format_rfc3339(timestamp: Timestamp) -> String {
    RFC3339_PRINTER.timestamp_with_offset_to_string(&timestamp, Offset::UTC)
}

pub fn format_rfc3339_millis(timestamp: Timestamp) -> String {
    RFC3339_MILLIS_PRINTER.timestamp_to_string(&timestamp)
}

pub fn format_rfc3339_with_offset(timestamp: Timestamp, offset: Offset) -> String {
    RFC3339_PRINTER.timestamp_with_offset_to_string(&timestamp, offset)
}

pub fn parse_rfc2822(input: &str) -> Result<Timestamp, jiff::Error> {
    RFC2822_PARSER.parse_timestamp(input)
}

#[allow(dead_code)]
pub fn format_rfc2822(timestamp: Timestamp) -> Result<String, jiff::Error> {
    RFC2822_PRINTER.zoned_to_string(&timestamp.to_zoned(TimeZone::UTC))
}

pub fn parse_clock_time(input: &str) -> Result<Time, jiff::Error> {
    Time::strptime("%H:%M:%S", input).or_else(|_| Time::strptime("%H:%M", input))
}

pub fn classify_civil_timestamp(
    time_zone: &TimeZone,
    datetime: DateTime,
) -> Result<CivilTimestamp, jiff::Error> {
    let ambiguous = time_zone.to_ambiguous_timestamp(datetime);
    match ambiguous.offset() {
        AmbiguousOffset::Unambiguous { offset } => {
            Ok(CivilTimestamp::Unambiguous(offset.to_timestamp(datetime)?))
        }
        AmbiguousOffset::Fold { before, .. } => {
            Ok(CivilTimestamp::Fold(before.to_timestamp(datetime)?))
        }
        AmbiguousOffset::Gap { .. } => Ok(CivilTimestamp::Gap),
    }
}

pub fn parse_user_cutoff(input: &str, now: &Zoned) -> Result<Option<Timestamp>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }

    if let Ok(timestamp) = parse_rfc3339(input) {
        return Ok(Some(timestamp));
    }

    let time = parse_clock_time(input)
        .map_err(|_| "Use HH:MM, HH:MM:SS, or an RFC3339 timestamp".to_string())?;
    let local_cutoff = now.date().to_datetime(time);
    match classify_civil_timestamp(now.time_zone(), local_cutoff)
        .map_err(|error| error.to_string())?
    {
        CivilTimestamp::Unambiguous(timestamp) | CivilTimestamp::Fold(timestamp) => {
            Ok(Some(timestamp))
        }
        CivilTimestamp::Gap => {
            Err("Cutoff time does not exist in the local timezone today".to_string())
        }
    }
}

pub fn format_user_cutoff_input(cutoff: Timestamp, now: &Zoned) -> String {
    let local_cutoff = cutoff.to_zoned(now.time_zone().clone());
    if local_cutoff.date() == now.date() {
        local_cutoff.strftime("%H:%M:%S").to_string()
    } else {
        format_rfc3339_with_offset(cutoff, local_cutoff.offset())
    }
}

pub fn format_user_cutoff_label(cutoff: Timestamp, time_zone: TimeZone) -> String {
    cutoff
        .to_zoned(time_zone)
        .strftime("%Y-%m-%d %H:%M:%S %Z")
        .to_string()
}

pub fn format_absolute_timestamp(timestamp: Timestamp) -> String {
    timestamp.strftime("%Y-%m-%d %H:%M:%S UTC").to_string()
}

pub fn elapsed_seconds(now: Timestamp, earlier: Timestamp) -> i64 {
    now.duration_since(earlier).as_secs()
}

pub fn elapsed_millis(now: Timestamp, earlier: Timestamp) -> i128 {
    now.duration_since(earlier).as_millis()
}

#[cfg(test)]
pub fn subtract_seconds(timestamp: Timestamp, seconds: i64) -> Timestamp {
    timestamp - SignedDuration::from_secs(seconds)
}

pub fn subtract_minutes(timestamp: Timestamp, minutes: i64) -> Timestamp {
    timestamp - SignedDuration::from_mins(minutes)
}

#[cfg(test)]
pub fn add_seconds(timestamp: Timestamp, seconds: i64) -> Timestamp {
    timestamp + SignedDuration::from_secs(seconds)
}

pub fn system_time_zone() -> TimeZone {
    system_time_zone_with(|| TimeZone::try_system().ok())
}

fn system_time_zone_with(discover: impl FnOnce() -> Option<TimeZone>) -> TimeZone {
    discover().unwrap_or(TimeZone::UTC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use serde::Serialize;

    #[test]
    fn parses_and_formats_rfc3339_with_existing_utc_offset() {
        let timestamp = parse_rfc3339("2026-05-12T11:00:00+01:00").unwrap();

        assert_eq!(format_rfc3339(timestamp), "2026-05-12T10:00:00+00:00");
    }

    #[test]
    fn parses_and_formats_rfc2822() {
        let timestamp = parse_rfc2822("Tue, 12 May 2026 11:00:00 +0100").unwrap();

        assert_eq!(
            format_rfc2822(timestamp).unwrap(),
            "Tue, 12 May 2026 10:00:00 +0000"
        );
    }

    #[test]
    fn timestamp_arithmetic_uses_signed_durations() {
        let timestamp = parse_rfc3339("2026-05-12T10:00:00Z").unwrap();

        assert_eq!(
            subtract_seconds(timestamp, 60),
            parse_rfc3339("2026-05-12T09:59:00Z").unwrap()
        );
        assert_eq!(
            elapsed_seconds(timestamp, subtract_seconds(timestamp, 61)),
            61
        );
    }

    #[test]
    fn timestamp_json_remains_an_rfc3339_string() {
        #[derive(Serialize)]
        struct Event {
            occurred_at: Timestamp,
        }

        let json = serde_json::to_string(&Event {
            occurred_at: parse_rfc3339("2026-05-12T10:00:00Z").unwrap(),
        })
        .unwrap();

        assert_eq!(json, r#"{"occurred_at":"2026-05-12T10:00:00Z"}"#);
    }

    #[test]
    fn local_cutoff_uses_the_supplied_zone_date() {
        let now = parse_rfc3339("2026-05-12T08:30:00Z")
            .unwrap()
            .to_zoned(TimeZone::get("Europe/London").unwrap());

        let cutoff = parse_user_cutoff("11:00", &now).unwrap().unwrap();

        assert_eq!(cutoff, parse_rfc3339("2026-05-12T10:00:00Z").unwrap());
    }

    #[test]
    fn fold_chooses_the_earlier_instant() {
        let zone = TimeZone::get("Europe/London").unwrap();
        let datetime = date(2026, 10, 25).at(1, 30, 0, 0);

        assert_eq!(
            classify_civil_timestamp(&zone, datetime).unwrap(),
            CivilTimestamp::Fold(parse_rfc3339("2026-10-25T00:30:00Z").unwrap())
        );
    }

    #[test]
    fn local_cutoff_in_a_fold_chooses_the_earlier_instant() {
        let now = parse_rfc3339("2026-10-25T12:00:00Z")
            .unwrap()
            .to_zoned(TimeZone::get("Europe/London").unwrap());

        let cutoff = parse_user_cutoff("01:30", &now).unwrap().unwrap();

        assert_eq!(cutoff, parse_rfc3339("2026-10-25T00:30:00Z").unwrap());
    }

    #[test]
    fn gap_is_rejected() {
        let zone = TimeZone::get("Europe/London").unwrap();
        let datetime = date(2026, 3, 29).at(1, 30, 0, 0);

        assert_eq!(
            classify_civil_timestamp(&zone, datetime).unwrap(),
            CivilTimestamp::Gap
        );
    }

    #[test]
    fn local_cutoff_in_a_gap_returns_an_error() {
        let now = parse_rfc3339("2026-03-29T12:00:00Z")
            .unwrap()
            .to_zoned(TimeZone::get("Europe/London").unwrap());

        let error = parse_user_cutoff("01:30", &now).unwrap_err();

        assert_eq!(
            error,
            "Cutoff time does not exist in the local timezone today"
        );
    }

    #[test]
    fn unavailable_system_time_zone_falls_back_to_utc() {
        assert_eq!(system_time_zone_with(|| None), TimeZone::UTC);
    }

    #[test]
    fn millisecond_format_has_exactly_three_digits_and_zulu_offset() {
        let timestamp = parse_rfc3339("2026-05-12T10:00:00.123456789Z").unwrap();

        assert_eq!(format_rfc3339_millis(timestamp), "2026-05-12T10:00:00.123Z");
    }
}
