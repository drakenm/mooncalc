//! Input validation shared by the services.

use crate::dto::{ApiError, CivilDate, Location};

fn is_leap_year(year: i32) -> bool {
    // Julian calendar before the 1582 Gregorian reform.
    if year < 1583 {
        year.rem_euclid(4) == 0
    } else {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// A real calendar date within years -9999..=9999. Whether the ephemeris
/// covers it is checked separately ([`crate::ephemeris::check_covers`]).
pub fn date(d: CivilDate) -> Result<(), ApiError> {
    if !(-9999..=9999).contains(&d.year) {
        return Err(ApiError::invalid_params(
            "year must be between -9999 and 9999",
        ));
    }
    if !(1..=12).contains(&d.month) {
        return Err(ApiError::invalid_params("month must be between 1 and 12"));
    }
    if d.day < 1 || d.day > days_in_month(d.year, d.month) {
        return Err(ApiError::invalid_params(format!(
            "{:04}-{:02} has no day {}",
            d.year, d.month, d.day
        )));
    }
    Ok(())
}

pub fn location(l: Location) -> Result<(), ApiError> {
    if !(-90.0..=90.0).contains(&l.lat_deg) {
        return Err(ApiError::invalid_params(
            "latDeg must be between -90 and 90",
        ));
    }
    if !(-180.0..=180.0).contains(&l.lon_deg) {
        return Err(ApiError::invalid_params(
            "lonDeg must be between -180 and 180",
        ));
    }
    if !(-500.0..=9000.0).contains(&l.elevation_m) {
        return Err(ApiError::invalid_params(
            "elevationM must be between -500 and 9000",
        ));
    }
    Ok(())
}

pub fn tz_offset(hours: f64) -> Result<(), ApiError> {
    if !(-14.0..=14.0).contains(&hours) {
        return Err(ApiError::invalid_params(
            "tzOffsetHours must be between -14 and +14",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(1500)); // Julian.
        assert!(is_leap_year(2024));
    }

    #[test]
    fn dates() {
        let d = |year, month, day| CivilDate { year, month, day };
        assert!(date(d(2024, 2, 29)).is_ok());
        assert!(date(d(2026, 2, 29)).is_err());
        assert!(date(d(2026, 13, 1)).is_err());
        assert!(date(d(2026, 4, 31)).is_err());
        assert!(date(d(10_000, 1, 1)).is_err());
    }
}
