//! What the Sky and Trajectory tabs show. The computation is shared with
//! the Tauri frontend in [`mooncalc_app::sky::scene`]: topocentric Sun and
//! Moon states ([`Snapshot`]), the evening or morning of a crescent
//! sighting ([`Night`], over [`NightScene`]) and one local day ([`Day`],
//! over [`DayScene`]). This module adds the cache keys built from the
//! GUI settings and [`ephemeris`], the single hook that picks the
//! ephemeris.

use std::ops::Deref;

use mooncalc_core::events::CalendarDate;
use mooncalc_core::{Ephemeris, GeoLocation};
use mooncalc_visibility::circumstances::Sighting;

pub use mooncalc_app::sky::scene::{
    ATMOSPHERE, BodyState, DAY_SAMPLE_STEP, DayScene, NIGHT_SAMPLE_STEP, NightScene, Snapshot,
    civil_noon, local_date, snapshot,
};

use crate::backend::BackendChoice;
use crate::settings::{Crescent, Settings};

/// The ephemeris used by the Sky and Trajectory tabs.
///
/// The shared backend from [`crate::backend::resolve`] (Meeus, or DE440
/// with its Meeus fallback). The scene keys include
/// `settings.backend.effective()`, so a backend change recomputes them.
pub fn ephemeris(settings: &Settings) -> std::sync::Arc<dyn Ephemeris> {
    crate::backend::resolve(settings.backend, &settings.de440_path).eph
}

/// Inputs that determine a [`Night`].
#[derive(Clone, Debug, PartialEq)]
pub struct NightKey {
    pub date: CalendarDate,
    pub tz_hours: f64,
    pub location: GeoLocation,
    pub crescent: Crescent,
    pub criterion_id: String,
    pub backend: BackendChoice,
}

impl NightKey {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            date: local_date(s.time.jd_ut, s.time.tz_offset_hours),
            tz_hours: s.time.tz_offset_hours,
            location: s.observer(),
            crescent: s.crescent,
            criterion_id: s.criterion_id.clone(),
            backend: s.backend.effective(),
        }
    }
}

/// The evening (or morning) of a crescent sighting on one local date: a
/// [`NightScene`] with the key it was computed for.
pub struct Night {
    pub key: NightKey,
    scene: NightScene,
}

impl Night {
    pub fn compute(key: NightKey, eph: &dyn Ephemeris) -> Self {
        let sighting = match key.crescent {
            Crescent::Evening => Sighting::Evening,
            Crescent::Morning => Sighting::Morning,
        };
        let scene = NightScene::compute(
            eph,
            key.date,
            key.tz_hours,
            key.location,
            sighting,
            &key.criterion_id,
        );
        Self { key, scene }
    }
}

impl Deref for Night {
    type Target = NightScene;
    fn deref(&self) -> &NightScene {
        &self.scene
    }
}

/// Inputs that determine a [`Day`].
#[derive(Clone, Debug, PartialEq)]
pub struct DayKey {
    pub date: CalendarDate,
    pub tz_hours: f64,
    pub location: GeoLocation,
    pub backend: BackendChoice,
}

impl DayKey {
    pub fn new(s: &Settings, jd_ut: f64) -> Self {
        Self {
            date: local_date(jd_ut, s.time.tz_offset_hours),
            tz_hours: s.time.tz_offset_hours,
            location: s.observer(),
            backend: s.backend.effective(),
        }
    }
}

/// One local calendar day for the Trajectory tab: a [`DayScene`] with the
/// key it was computed for.
pub struct Day {
    pub key: DayKey,
    scene: DayScene,
}

impl Day {
    pub fn compute(key: DayKey, eph: &dyn Ephemeris) -> Self {
        let scene = DayScene::compute(eph, key.date, key.tz_hours, key.location);
        Self { key, scene }
    }
}

impl Deref for Day {
    type Target = DayScene;
    fn deref(&self) -> &DayScene {
        &self.scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::angle::DEG;
    use mooncalc_core::events::phases::Phase;
    use mooncalc_core::{Instant, MeeusEphemeris};

    fn mecca() -> GeoLocation {
        GeoLocation::new(21.4225, 39.8262, 277.0)
    }

    fn night_key(date: CalendarDate, loc: GeoLocation, tz: f64) -> NightKey {
        NightKey {
            date,
            tz_hours: tz,
            location: loc,
            crescent: Crescent::Evening,
            criterion_id: "yallop".into(),
            backend: BackendChoice::Meeus,
        }
    }

    #[test]
    fn young_crescent_evening_at_mecca() {
        // New Moon 2024 March 10, 09:00 UT. On the evening of March 11 at
        // Mecca the ~1.5-day crescent sets well after the Sun.
        let eph = MeeusEphemeris::new();
        let night = Night::compute(
            night_key(CalendarDate::new(2024, 3, 11), mecca(), 3.0),
            &eph,
        );
        let (ss, ms) = (night.sun_event.unwrap(), night.moon_event.unwrap());
        // Sunset about 18:35 local (15:35 UT).
        let local = (ss + 3.0 / 24.0 + 0.5).fract() * 24.0;
        assert!((18.3..18.9).contains(&local), "sunset {local} h");
        assert!(ms > ss + 1.0 / 24.0, "lag {} min", (ms - ss) * 1440.0);
        assert!(night.start < ss && night.end > ms);
        assert!(night.samples.len() > 60);
        let t = night.default_time();
        let snap = night.snapshot(t);
        // Thin evening crescent above the western horizon, Sun below.
        assert!(
            snap.illumination.fraction < 0.06,
            "{}",
            snap.illumination.fraction
        );
        assert!(snap.moon.alt > 0.0 && snap.sun.alt < 0.0);
        assert!((240.0..300.0).contains(&(snap.moon.az / DEG)));
        // The lit limb points down toward the Sun: χ − q near 180° (the
        // Sun is below and slightly to one side).
        let zeta = snap.limb_zenith / DEG;
        assert!((120.0..240.0).contains(&zeta), "χ−q = {zeta}");
        // And the geometry agrees with the circumstances at their instant.
        let g = night.geometry(night.evaluation.unwrap()).unwrap();
        let g0 = night.outcome.geometry().unwrap();
        assert!((g.arcv_deg - g0.arcv_deg).abs() < 1e-9);
        assert!((g.age_hours - g0.age_hours).abs() < 1e-6);
        assert!(snap.elongation_lon_deg < 40.0);
    }

    #[test]
    fn morning_crescent_span_precedes_sunrise() {
        // Old crescent on the morning of 2024 March 8 at Mecca.
        let eph = MeeusEphemeris::new();
        let mut key = night_key(CalendarDate::new(2024, 3, 8), mecca(), 3.0);
        key.crescent = Crescent::Morning;
        let night = Night::compute(key, &eph);
        let (sr, mr) = (night.sun_event.unwrap(), night.moon_event.unwrap());
        assert!(mr < sr);
        assert!(night.start < mr && night.end > sr);
        let snap = night.snapshot(night.default_time());
        assert!((60.0..130.0).contains(&(snap.moon.az / DEG)));
        assert!(snap.elongation_lon_deg > 320.0);
    }

    #[test]
    fn day_events_in_london() {
        let eph = MeeusEphemeris::new();
        let key = DayKey {
            date: CalendarDate::new(2024, 3, 11),
            tz_hours: 0.0,
            location: GeoLocation::new(51.5074, -0.1278, 11.0),
            backend: BackendChoice::Meeus,
        };
        let day = Day::compute(key, &eph);
        assert_eq!(day.samples.len(), 289);
        // Sunrise 06:20, sunset 18:00 UT (HMNAO/timeanddate: 06:19, 18:00).
        let h = |t: Instant| (t.jd_ut - day.start) * 24.0;
        assert!((h(day.sun.rise.unwrap()) - 6.33).abs() < 0.1);
        assert!((h(day.sun.set.unwrap()) - 18.0).abs() < 0.1);
        // The young Moon rises in the morning after the Sun and sets after it.
        let mr = h(day.moon.rise.unwrap());
        let ms = h(day.moon.set.unwrap());
        assert!(mr > 6.3 && mr < 9.0, "moonrise {mr}");
        assert!(ms > 19.0 && ms < 23.0, "moonset {ms}");
        // Phases around the day.
        let nm = day.next_phase(Phase::New, day.start).unwrap();
        assert!((nm.jd_ut - day.start - 28.6).abs() < 1.0);
        let age = day.moon_age_days(day.start + 0.5).unwrap();
        assert!((age - 1.13).abs() < 0.1, "{age}");
        let full = day.next_phase(Phase::Full, day.start).unwrap();
        // Full Moon 2024 March 25, 07:00 UT.
        assert!((full.jd_ut - (day.start + 14.29)).abs() < 0.1);
        assert!(day.twilights[0].1.unwrap().jd_ut > day.sun.set.unwrap().jd_ut);
    }
}
