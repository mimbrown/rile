//! Dates formatted as `strftime` does in the C locale (SILE's `date`
//! package, which formats with Lua's `os.date`).

use std::fmt::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
];

/// A moment as read on a clock `offset` seconds ahead of UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateError(pub String);

impl std::fmt::Display for DateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid conversion specifier '%{}'", self.0)
    }
}

impl std::error::Error for DateError {}

impl DateTime {
    /// `seconds` after 1970-01-01T00:00:00Z, on a clock `offset` seconds
    /// ahead of UTC.
    pub fn from_unix(seconds: i64, offset: i32) -> Self {
        let local = seconds + offset as i64;
        let (days, secs) = (local.div_euclid(86_400), local.rem_euclid(86_400) as u32);
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day, hour: secs / 3600, minute: secs / 60 % 60, second: secs % 60, offset }
    }

    pub fn now_utc() -> Self {
        let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(e) => -(e.duration().as_secs() as i64),
        };
        Self::from_unix(seconds, 0)
    }

    fn days(&self) -> i64 {
        let y = if self.month <= 2 { self.year - 1 } else { self.year };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (self.month as i64 + 9) % 12;
        let doy = (153 * mp + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// 0 for Sunday.
    pub fn weekday(&self) -> u32 {
        (self.days() + 4).rem_euclid(7) as u32
    }

    /// 1 for the first of January.
    pub fn day_of_year(&self) -> u32 {
        let start = DateTime { month: 1, day: 1, ..*self };
        (self.days() - start.days()) as u32 + 1
    }

    /// Format as C's `strftime` in the C locale, as SILE's `\date` does
    /// where the document language's locale isn't installed.
    pub fn format(&self, format: &str) -> Result<String, DateError> {
        let mut out = String::new();
        let mut chars = format.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            let Some(spec) = chars.next() else { return Err(DateError(String::new())) };
            let hour12 = (self.hour + 11) % 12 + 1;
            let _ = match spec {
                'a' => write!(out, "{}", &DAYS[self.weekday() as usize][..3]),
                'A' => write!(out, "{}", DAYS[self.weekday() as usize]),
                'b' | 'h' => write!(out, "{}", &MONTHS[self.month as usize - 1][..3]),
                'B' => write!(out, "{}", MONTHS[self.month as usize - 1]),
                'c' => write!(out, "{}", self.format("%a %b %e %H:%M:%S %Y")?),
                'C' => write!(out, "{:02}", self.year.div_euclid(100)),
                'd' => write!(out, "{:02}", self.day),
                'D' | 'x' => write!(out, "{}", self.format("%m/%d/%y")?),
                'e' => write!(out, "{:2}", self.day),
                'F' => write!(out, "{}", self.format("%Y-%m-%d")?),
                'H' => write!(out, "{:02}", self.hour),
                'I' => write!(out, "{hour12:02}"),
                'j' => write!(out, "{:03}", self.day_of_year()),
                'm' => write!(out, "{:02}", self.month),
                'M' => write!(out, "{:02}", self.minute),
                'n' => out.write_char('\n'),
                'p' => write!(out, "{}", if self.hour < 12 { "AM" } else { "PM" }),
                'r' => write!(out, "{}", self.format("%I:%M:%S %p")?),
                'R' => write!(out, "{}", self.format("%H:%M")?),
                'S' => write!(out, "{:02}", self.second),
                't' => out.write_char('\t'),
                'T' | 'X' => write!(out, "{}", self.format("%H:%M:%S")?),
                'u' => write!(out, "{}", (self.weekday() + 6) % 7 + 1),
                'w' => write!(out, "{}", self.weekday()),
                'y' => write!(out, "{:02}", self.year.rem_euclid(100)),
                'Y' => write!(out, "{}", self.year),
                'z' => {
                    let sign = if self.offset < 0 { '-' } else { '+' };
                    let minutes = self.offset.unsigned_abs() / 60;
                    write!(out, "{sign}{:02}{:02}", minutes / 60, minutes % 60)
                }
                '%' => out.write_char('%'),
                other => return Err(DateError(other.to_string())),
            };
        }
        Ok(out)
    }
}

/// Year, month and day of `days` after 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_c_locale() {
        let date = DateTime::from_unix(1_791_010_368, 0);
        assert_eq!(date.format("%c").unwrap(), "Sat Oct  3 06:52:48 2026");
        assert_eq!(date.format("%A, %d %B").unwrap(), "Saturday, 03 October");
        assert_eq!(date.format("%F %j %u %w %I%p %%").unwrap(), "2026-10-03 276 6 6 06AM %");
        assert!(date.format("%Q").is_err());
    }

    #[test]
    fn offsets_move_the_clock() {
        let date = DateTime::from_unix(0, -3600);
        assert_eq!(date.format("%Y-%m-%d %H:%M %z").unwrap(), "1969-12-31 23:00 -0100");
        assert_eq!(DateTime::from_unix(951_782_400, 0).format("%F %a").unwrap(), "2000-02-29 Tue");
    }
}
