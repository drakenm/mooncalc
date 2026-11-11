//! Shared machinery of the **Sky** and **Trajectory** tabs.
//!
//! * [`geometry`]: pure projection math (horizon panorama, all-sky,
//!   compass, star chart), screen directions and sky colours, plus the
//!   crescent's orientation (parallactic angle, bright limb from the
//!   zenith) and small helpers re-exported from `mooncalc-app`. Unit-tested.
//! * [`scene`]: the evening or morning of a crescent sighting
//!   ([`scene::Night`]) and one local day ([`scene::Day`]), computed by
//!   [`mooncalc_app::sky::scene`] and keyed by the GUI settings;
//!   [`scene::ephemeris`] is the single hook that picks the ephemeris.
//! * [`stars`]: the embedded bright-star list (Yale BSC5, V < 3.5) and its
//!   reduction to the equinox of date, from [`mooncalc_app::sky::stars`].
//! * [`draw`]: egui painter helpers (phased Moon, Sun, stars, gradients).
//!
//! The frontend-independent parts live in [`mooncalc_app::sky`], shared
//! with the Tauri frontend.

pub mod draw;
pub mod geometry;
pub mod scene;
pub use mooncalc_app::sky::stars;

use crate::datetime::DateTimeFields;

/// `HH:MM` local time of a UT Julian Day in a zone `tz_hours` ahead of UT.
pub fn format_hm(jd_ut: f64, tz_hours: f64) -> String {
    let f = DateTimeFields::from_jd(jd_ut + tz_hours / 24.0);
    format!("{:02}:{:02}", f.hour, f.minute)
}

/// `YYYY-MM-DD HH:MM` local time of a UT Julian Day.
pub fn format_date_hm(jd_ut: f64, tz_hours: f64) -> String {
    DateTimeFields::from_jd(jd_ut + tz_hours / 24.0).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_time_formatting() {
        // 2024-03-11 15:35 UT is 18:35 in Mecca (UTC+3).
        let jd = crate::datetime::calendar_to_jd(2024, 3, 11.0) + (15.0 + 35.0 / 60.0) / 24.0;
        assert_eq!(format_hm(jd, 3.0), "18:35");
        assert_eq!(format_hm(jd, -5.5), "10:05");
        assert_eq!(format_date_hm(jd, 10.0), "2024-03-12 01:35");
    }
}
