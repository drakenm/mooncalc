//! Islamic (Hijri) calendars for MoonCalc.
//!
//! Several calendars to choose from, all behind the [`HijriCalendar`] trait:
//!
//! * **Tabular** (arithmetical) calendars: the four classical 30-year
//!   intercalation schemes, each with the civil (Friday) and astronomical
//!   (Thursday) epoch. See [`tabular`].
//! * **Umm al-Qura**, the official calendar of Saudi Arabia, table-driven:
//!   the officially published calendar for 1343–1500 AH and the ICU/CLDR
//!   table for 1300–1600 AH. See [`umalqura`].
//! * **Observation- or rule-based** calendars built on a
//!   [`MonthStartProvider`]: crescent visibility (supplied by the visibility
//!   engine), conjunction rules ([`newmoon`]), or announced dates
//!   ([`TableProvider`]). See [`observed`].
//!
//! [`calendars()`] lists the built-in calendars for a selector, and [`grid`]
//! builds month views.
//!
//! # Day convention
//!
//! The Islamic day begins at sunset. Conversions here follow the civil-day
//! convention: a Hijri date corresponds to the civil (midnight-to-midnight)
//! day that begins after its first sunset, and civil days are given as
//! integer Julian Day Numbers (the Julian Date at noon of that civil day).
//! For example, 1 Ramaḍān 1445 in Umm al-Qura is Monday 11 March 2024
//! (JDN 2 460 381), although the month began at sunset on 10 March.
//!
//! # Portability
//!
//! The crate has no dependencies beyond `std` collections and a `Mutex`,
//! does no I/O, and builds for `wasm32-unknown-unknown`.
//!
//! # Example
//!
//! ```
//! use mooncalc_hijri::civil::{CivilCalendar, CivilDate};
//! use mooncalc_hijri::{HijriCalendar, HijriDate, UMM_AL_QURA};
//!
//! let jdn = CivilDate::new(2024, 7, 7).to_jdn(CivilCalendar::Gregorian).unwrap();
//! assert_eq!(UMM_AL_QURA.to_hijri(jdn), Some(HijriDate::new(1446, 1, 1)));
//! ```

pub mod calendar;
pub mod civil;
pub mod date;
pub mod grid;
pub mod newmoon;
pub mod observed;
pub mod registry;
pub mod tabular;
pub mod umalqura;

pub use calendar::{HijriCalendar, SupportedRange};
pub use civil::{CivilCalendar, CivilDate};
pub use date::{HijriDate, HijriMonth, Weekday};
pub use grid::{GridDay, MonthGrid, civil_month_grid, hijri_month_grid};
pub use newmoon::{NewMoonRule, NewMoonRuleProvider};
pub use observed::{CalendarProvider, MonthStartProvider, ObservedCalendar, TableProvider};
pub use registry::{
    CalendarInfo, CalendarKind, DEFAULT_CALENDAR_ID, calendar_by_id, calendars, info_by_id,
};
pub use tabular::{Intercalation, TabularCalendar, TabularEpoch};
pub use umalqura::{TableCalendar, UMM_AL_QURA, UMM_AL_QURA_ICU};
