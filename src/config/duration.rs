use crate::error::FrostxError;
use chrono::{DateTime, Datelike, Months, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A project-inactivity duration parsed from strings like `"90d"`, `"6m"`, `"1y"`, `"12h"`.
///
/// Months and years are calendar-based (same day next month / next year).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Duration {
    pub value: u32,
    pub unit: DurationUnit,
}

/// Unit component of a [`Duration`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurationUnit {
    /// Hours (`h`).
    Hours,
    /// Days (`d`).
    Days,
    /// Weeks (`w`).
    Weeks,
    /// Calendar months (`m`).
    Months,
    /// Calendar years (`y`).
    Years,
}

impl Duration {
    /// Returns true if `elapsed` since `last_activity` is at least this duration.
    pub fn has_elapsed_since(&self, last_activity: DateTime<Utc>) -> bool {
        let now = Utc::now();
        let threshold = self.apply_to(last_activity);
        now >= threshold
    }

    /// Returns the remaining seconds until this duration elapses from `last_activity`,
    /// or 0 if it has already elapsed.
    pub fn remaining_seconds_from(&self, last_activity: DateTime<Utc>) -> i64 {
        let threshold = self.apply_to(last_activity);
        let now = Utc::now();
        (threshold - now).num_seconds().max(0)
    }

    fn apply_to(&self, base: DateTime<Utc>) -> DateTime<Utc> {
        match self.unit {
            DurationUnit::Hours => base + chrono::Duration::hours(i64::from(self.value)),
            DurationUnit::Days => base + chrono::Duration::days(i64::from(self.value)),
            DurationUnit::Weeks => base + chrono::Duration::weeks(i64::from(self.value)),
            DurationUnit::Months => base
                .checked_add_months(Months::new(self.value))
                .unwrap_or(DateTime::<Utc>::MAX_UTC),
            DurationUnit::Years => base
                .with_year(base.year() + i32::try_from(self.value).unwrap_or(i32::MAX))
                .unwrap_or(DateTime::<Utc>::MAX_UTC),
        }
    }

    /// Parse from a string like `"90d"`, `"6m"`, `"1y"`, `"24h"`, `"2w"`.
    pub fn parse(s: &str) -> Result<Self, FrostxError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(FrostxError::Config("empty duration string".into()));
        }
        let (num_str, unit_char) = s.split_at(s.len() - 1);
        let value: u32 = num_str.parse().map_err(|_| {
            FrostxError::Config(format!(
                "invalid duration '{s}': expected format like \"90d\", \"6m\", \"1y\""
            ))
        })?;
        if value == 0 {
            return Err(FrostxError::Config(format!(
                "invalid duration '{s}': value must be > 0"
            )));
        }
        let unit = match unit_char {
            "h" | "hour"  | "hours"  => DurationUnit::Hours,
            "d" | "day"   | "days"   => DurationUnit::Days,
            "w" | "week"  | "weeks"  => DurationUnit::Weeks,
            "m" | "month" | "months" => DurationUnit::Months,
            "y" | "year"  | "years"  => DurationUnit::Years,
            other => {
                return Err(FrostxError::Config(format!(
                    "invalid duration unit '{other}' in '{s}': expected h, d, w, m, or y"
                )))
            }
        };
        Ok(Self { value, unit })
    }
}

impl TryFrom<String> for Duration {
    type Error = FrostxError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<Duration> for String {
    fn from(d: Duration) -> Self {
        d.to_string()
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self.unit {
            DurationUnit::Hours => "h",
            DurationUnit::Days => "d",
            DurationUnit::Weeks => "w",
            DurationUnit::Months => "mon",
            DurationUnit::Years => "y",
        };
        write!(f, "{}{}", self.value, unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_days() {
        let d = Duration::parse("90d").unwrap();
        assert_eq!(d.value, 90);
        assert_eq!(d.unit, DurationUnit::Days);
    }

    #[test]
    fn parse_months() {
        let d = Duration::parse("6m").unwrap();
        assert_eq!(d.value, 6);
        assert_eq!(d.unit, DurationUnit::Months);
    }

    #[test]
    fn parse_years() {
        let d = Duration::parse("1y").unwrap();
        assert_eq!(d.value, 1);
        assert_eq!(d.unit, DurationUnit::Years);
    }

    #[test]
    fn parse_hours() {
        let d = Duration::parse("24h").unwrap();
        assert_eq!(d.value, 24);
        assert_eq!(d.unit, DurationUnit::Hours);
    }

    #[test]
    fn parse_weeks() {
        let d = Duration::parse("2w").unwrap();
        assert_eq!(d.value, 2);
        assert_eq!(d.unit, DurationUnit::Weeks);
    }

    #[test]
    fn reject_zero() {
        assert!(Duration::parse("0d").is_err());
    }

    #[test]
    fn reject_bad_unit() {
        assert!(Duration::parse("30x").is_err());
    }

    #[test]
    fn reject_empty() {
        assert!(Duration::parse("").is_err());
    }

    #[test]
    fn roundtrip_display() {
        for s in &["90d", "6m", "1y", "24h", "2w"] {
            assert_eq!(Duration::parse(s).unwrap().to_string(), *s);
        }
    }

    #[test]
    fn elapsed_in_past() {
        let ancient = Utc::now() - chrono::Duration::days(200);
        let d = Duration::parse("90d").unwrap();
        assert!(d.has_elapsed_since(ancient));
    }

    #[test]
    fn not_elapsed_recently() {
        let recent = Utc::now() - chrono::Duration::days(10);
        let d = Duration::parse("90d").unwrap();
        assert!(!d.has_elapsed_since(recent));
    }

    #[test]
    fn remaining_seconds_positive() {
        let recent = Utc::now() - chrono::Duration::days(10);
        let d = Duration::parse("90d").unwrap();
        assert!(d.remaining_seconds_from(recent) > 0);
    }

    #[test]
    fn remaining_seconds_zero_when_elapsed() {
        let ancient = Utc::now() - chrono::Duration::days(200);
        let d = Duration::parse("90d").unwrap();
        assert_eq!(d.remaining_seconds_from(ancient), 0);
    }
}
