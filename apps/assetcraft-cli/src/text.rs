//! Human-readable output: ISO-8601 dates without a date crate, byte sizes, and plain-ASCII
//! tables that line up on Windows consoles.

/// Latest time we format as a date (9999-12-31T23:59:59Z); anything later prints as `@secs`.
const MAX_FORMATTED_SECS: u64 = 253_402_300_799;

/// Days since 1970-01-01 to a proleptic Gregorian `(year, month, day)`.
/// Howard Hinnant's `civil_from_days` (public domain algorithm).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, u32::try_from(month).unwrap_or(1), u32::try_from(day).unwrap_or(1))
}

/// Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub fn iso_utc(secs: u64) -> String {
    if secs > MAX_FORMATTED_SECS {
        return format!("@{secs}");
    }
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Bytes as `512 B`, `1.5 KB`, `12.0 MB` … (1024-based, like Explorer).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Precision loss above 2^53 bytes is irrelevant for one decimal place.
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Column alignment.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Pad on the right.
    Left,
    /// Pad on the left (numbers).
    Right,
}

/// A table with a header, a dashed rule and two spaces between columns. Widths count `char`s,
/// so accented names line up; the last column is never padded (no trailing spaces).
pub fn table(headers: &[&str], align: &[Align], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(cols) {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let line = |cells: &mut dyn Iterator<Item = &str>| {
        let mut out = String::new();
        for (i, cell) in cells.enumerate().take(cols) {
            if i > 0 {
                out.push_str("  ");
            }
            let pad = widths[i].saturating_sub(cell.chars().count());
            let right = align.get(i) == Some(&Align::Right);
            if right {
                out.push_str(&" ".repeat(pad));
            }
            out.push_str(cell);
            if !right && i + 1 < cols {
                out.push_str(&" ".repeat(pad));
            }
        }
        out.truncate(out.trim_end().len());
        out.push('\n');
        out
    };
    let mut out = line(&mut headers.iter().copied());
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&line(&mut rule.iter().map(String::as_str)));
    for row in rows {
        out.push_str(&line(&mut row.iter().map(String::as_str)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_match_known_values() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(19_783), (2024, 3, 1));
    }

    #[test]
    fn iso_utc_formats_and_caps() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_utc(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(iso_utc(253_402_300_799), "9999-12-31T23:59:59Z");
        assert_eq!(iso_utc(u64::MAX), format!("@{}", u64::MAX));
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(12 * 1024 * 1024), "12.0 MB");
        assert_eq!(human_size(u64::MAX), "16384.0 PB");
    }

    #[test]
    fn table_aligns_by_chars_without_trailing_spaces() {
        let t = table(
            &["KIND", "SIZE", "NAME"],
            &[Align::Left, Align::Right, Align::Left],
            &[vec!["DIR".into(), "-".into(), "Réaper".into()], vec![String::new(), "10 B".into(), "a".into()]],
        );
        assert_eq!(t, "KIND  SIZE  NAME\n----  ----  ------\nDIR      -  Réaper\n      10 B  a\n");
    }
}
