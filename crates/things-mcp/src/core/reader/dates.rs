//! Date helpers for the read path.
//!
//! Things stores user-facing dates (`startDate`, `deadline`) as bit-packed
//! integers:
//!
//! ```text
//!   bit  26                              0
//!        YYYYYYYYYYY MMMM DDDDD 0000000
//!        ↑ 11 bits   ↑ 4   ↑ 5   ↑ 7 bits padding
//! ```
//!
//! `0` means "no date" (Things never writes `1970-00-00`). Things stores
//! row-modification timestamps (`creationDate`, `userModificationDate`,
//! `stopDate`) separately as REAL Unix seconds — those go through
//! `unix_to_iso` over in `queries.rs`, not here.

/// Decode a Things packed date into an ISO `YYYY-MM-DD` string.
/// Returns `None` for `0` and for out-of-range / malformed values so a
/// future schema change can't surface garbage to callers.
pub fn decode_things_date(packed: i64) -> Option<String> {
    if packed == 0 {
        return None;
    }
    let year = ((packed >> 16) & 0xFFF) as i32;
    let month = ((packed >> 12) & 0x0F) as u32;
    let day = ((packed >> 7) & 0x1F) as u32;
    if year < 1900 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Pack a `(year, month, day)` triple back into Things' format. Used by
/// query helpers that need to compare against `today` or user-supplied
/// date bounds without round-tripping through ISO strings.
pub fn pack_things_date(year: i32, month: u32, day: u32) -> i64 {
    ((year as i64) << 16) | ((month as i64) << 12) | ((day as i64) << 7)
}

/// Parse `YYYY-MM-DD` into `(y, m, d)`. Returns `None` on any deviation
/// from the strict 10-character ISO date form so caller validation is
/// straightforward.
pub fn parse_iso_date(iso: &str) -> Option<(i32, u32, u32)> {
    if iso.len() != 10 {
        return None;
    }
    let bytes = iso.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i32 = iso.get(0..4)?.parse().ok()?;
    let m: u32 = iso.get(5..7)?.parse().ok()?;
    let d: u32 = iso.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// `(year, month, day)` → Unix epoch seconds (00:00 UTC of that date).
/// Inverse of `core::backup::__test_only_unix_to_ymdhms` rounded to whole
/// days. Used by `things_list_logbook`'s `from`/`to` filters which compare
/// against `stopDate` (REAL Unix seconds).
pub fn ymd_to_unix_utc(year: i32, month: u32, day: u32) -> i64 {
    let mut days: i64 = 0;
    let from_year = 1970;
    if year >= from_year {
        for yi in from_year..year {
            let leap = (yi % 4 == 0 && yi % 100 != 0) || (yi % 400 == 0);
            days += if leap { 366 } else { 365 };
        }
    } else {
        for yi in year..from_year {
            let leap = (yi % 4 == 0 && yi % 100 != 0) || (yi % 400 == 0);
            days -= if leap { 366 } else { 365 };
        }
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let months_len: [u32; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for mo in 1..month {
        days += months_len[(mo - 1) as usize] as i64;
    }
    days += (day - 1) as i64;
    days * 86_400
}

/// Today's date (UTC), packed for direct comparison against Things'
/// `startDate` / `deadline` columns.
pub fn today_packed_utc() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d, _, _, _) = crate::core::backup::__test_only_unix_to_ymdhms(secs);
    pack_things_date(y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_zero_is_none() {
        assert_eq!(decode_things_date(0), None);
    }

    #[test]
    fn decode_known_packed_value() {
        // pack(2026, 5, 20)
        let p = pack_things_date(2026, 5, 20);
        assert_eq!(decode_things_date(p), Some("2026-05-20".to_string()));
    }

    #[test]
    fn pack_then_decode_round_trip() {
        for (y, m, d) in [(2000, 1, 1), (2026, 12, 31), (2099, 12, 31)] {
            let p = pack_things_date(y, m, d);
            assert_eq!(
                decode_things_date(p),
                Some(format!("{y:04}-{m:02}-{d:02}"))
            );
        }
    }

    #[test]
    fn decode_rejects_malformed_year() {
        // Year 1800 — below the 1900 sanity cutoff, mark as malformed.
        let p = pack_things_date(1800, 1, 1);
        assert_eq!(decode_things_date(p), None);
    }

    #[test]
    fn parse_iso_date_happy_path() {
        assert_eq!(parse_iso_date("2026-05-20"), Some((2026, 5, 20)));
    }

    #[test]
    fn parse_iso_date_rejects_wrong_separators() {
        assert_eq!(parse_iso_date("2026/05/20"), None);
    }

    #[test]
    fn parse_iso_date_rejects_wrong_length() {
        assert_eq!(parse_iso_date("2026-5-20"), None);
        assert_eq!(parse_iso_date("2026-05-2"), None);
    }

    #[test]
    fn ymd_to_unix_utc_at_epoch() {
        assert_eq!(ymd_to_unix_utc(1970, 1, 1), 0);
        assert_eq!(ymd_to_unix_utc(1970, 1, 2), 86_400);
    }

    #[test]
    fn ymd_to_unix_utc_leap_day() {
        // 2024-02-29 → days = 365*54 + 13 leap days + 31 (jan) + 28 (feb) days
        // We don't hard-code the answer; just check that 2024-03-01 is one day
        // after 2024-02-29.
        let a = ymd_to_unix_utc(2024, 2, 29);
        let b = ymd_to_unix_utc(2024, 3, 1);
        assert_eq!(b - a, 86_400);
    }

    #[test]
    fn today_packed_utc_decodes_to_real_date() {
        let p = today_packed_utc();
        let s = decode_things_date(p).expect("today must decode");
        assert_eq!(s.len(), 10);
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
    }
}
