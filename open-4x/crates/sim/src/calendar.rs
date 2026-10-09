//! Proleptic Gregorian calendar using integer arithmetic only. No clock, locale, or timezone.
//!
//! Scenarios start on an explicit calendar date and one campaign turn is one day. The
//! civil-date conversions follow Howard Hinnant's public-domain `days_from_civil` algorithms.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
pub const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// A valid calendar day between years 1 and 9999. Serialized as ISO-8601 `YYYY-MM-DD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

impl Date {
    pub const MIN_YEAR: i32 = 1;
    pub const MAX_YEAR: i32 = 9999;

    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        let valid = (Self::MIN_YEAR..=Self::MAX_YEAR).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= Self::days_in_month(year, month);
        valid.then_some(Self { year, month, day })
    }
    pub fn year(self) -> i32 {
        self.year
    }
    pub fn month(self) -> u8 {
        self.month
    }
    pub fn day(self) -> u8 {
        self.day
    }
    pub fn is_leap_year(year: i32) -> bool {
        year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
    }
    pub fn days_in_month(year: i32, month: u8) -> u8 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if Self::is_leap_year(year) => 29,
            _ => 28,
        }
    }
    /// Days since 1970-01-01. Negative before the Unix epoch.
    pub fn to_days(self) -> i64 {
        let m = i64::from(self.month);
        let d = i64::from(self.day);
        let y = i64::from(self.year) - i64::from(m <= 2);
        let era = y.div_euclid(400);
        let year_of_era = y - era * 400;
        let shifted_month = (m + 9) % 12;
        let day_of_year = (153 * shifted_month + 2) / 5 + d - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }
    /// Inverse of [`Date::to_days`]. Returns `None` outside years 1–9999.
    pub fn from_days(days: i64) -> Option<Self> {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Self::new(i32::try_from(year).ok()?, month as u8, day as u8)
    }
    /// Saturates at the last supported day instead of overflowing.
    pub fn add_days(self, days: u32) -> Self {
        Self::from_days(self.to_days() + i64::from(days)).unwrap_or(Self {
            year: Self::MAX_YEAR,
            month: 12,
            day: 31,
        })
    }
    pub fn weekday(self) -> &'static str {
        // 1970-01-01 was a Thursday.
        WEEKDAYS[(self.to_days() + 4).rem_euclid(7) as usize]
    }
    pub fn month_name(self) -> &'static str {
        MONTHS[usize::from(self.month) - 1]
    }
    /// `YYYY-MM-DD`.
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
    /// Strict `YYYY-MM-DD`: four-digit year, two-digit month and day.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let digits = |range: std::ops::Range<usize>| {
            let part = &text[range];
            part.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| part.parse::<i32>().ok())
                .flatten()
        };
        Self::new(digits(0..4)?, digits(5..7)? as u8, digits(8..10)? as u8)
    }
}

/// `1 January 1876`.
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.day, self.month_name(), self.year)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.iso())
    }
}
impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid date {text:?}; use YYYY-MM-DD"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_known_dates() {
        assert_eq!(Date::new(1970, 1, 1).unwrap().to_days(), 0);
        assert_eq!(Date::new(1876, 1, 1).unwrap().to_days(), -34_333);
        assert_eq!(Date::new(2000, 3, 1).unwrap().to_days(), 11_017);
    }
    #[test]
    fn start_of_1876_was_a_saturday() {
        let start = Date::new(1876, 1, 1).unwrap();
        assert_eq!(start.weekday(), "Saturday");
        assert_eq!(start.to_string(), "1 January 1876");
        assert_eq!(start.iso(), "1876-01-01");
    }
    #[test]
    fn leap_years_follow_gregorian_rules() {
        assert!(Date::is_leap_year(1876));
        assert!(!Date::is_leap_year(1900));
        assert!(Date::is_leap_year(2000));
        assert!(Date::new(1876, 2, 29).is_some());
        assert!(Date::new(1877, 2, 29).is_none());
        assert!(Date::new(1900, 2, 29).is_none());
        let start = Date::new(1876, 1, 1).unwrap();
        // 1876 has 366 days, so day 60 is the leap day and day 367 is New Year 1877.
        assert_eq!(start.add_days(59), Date::new(1876, 2, 29).unwrap());
        assert_eq!(start.add_days(60), Date::new(1876, 3, 1).unwrap());
        assert_eq!(start.add_days(366), Date::new(1877, 1, 1).unwrap());
    }
    #[test]
    fn days_roundtrip_across_centuries() {
        // 0001-01-01 is day -719162 and 9999-12-31 is day 2932896.
        assert_eq!(Date::new(1, 1, 1).unwrap().to_days(), -719_162);
        assert_eq!(Date::new(9999, 12, 31).unwrap().to_days(), 2_932_896);
        assert!(Date::from_days(-719_163).is_none() && Date::from_days(2_932_897).is_none());
        for days in (-719_162..=2_932_896).step_by(997) {
            let date = Date::from_days(days).unwrap();
            assert_eq!(date.to_days(), days, "{date}");
        }
        for days in -34_000..-33_000 {
            let date = Date::from_days(days).unwrap();
            assert_eq!(Date::parse(&date.iso()), Some(date));
        }
    }
    #[test]
    fn strict_parsing_rejects_ambiguous_text() {
        assert_eq!(Date::parse("1876-01-01"), Date::new(1876, 1, 1));
        for bad in [
            "1876-1-1",
            "76-01-01",
            "1876/01/01",
            "1876-13-01",
            "1876-00-10",
            "1876-02-30",
            "+876-01-01",
            "1876-01-01 ",
            "",
            "0000-01-01",
        ] {
            assert!(Date::parse(bad).is_none(), "{bad:?}");
        }
    }
    #[test]
    fn serde_uses_iso_strings_and_saturates() {
        let date = Date::new(1876, 1, 1).unwrap();
        assert_eq!(serde_json::to_string(&date).unwrap(), "\"1876-01-01\"");
        assert_eq!(
            serde_json::from_str::<Date>("\"1876-01-01\"").unwrap(),
            date
        );
        assert!(serde_json::from_str::<Date>("\"1876-02-30\"").is_err());
        assert_eq!(
            Date::new(9999, 12, 30).unwrap().add_days(500),
            Date::new(9999, 12, 31).unwrap()
        );
    }
}
