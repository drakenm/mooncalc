//! Islamic (Hijri) calendars for MoonCalc.
//!
//! # Day convention
//!
//! The Islamic day begins at sunset. Conversions here follow the civil-day
//! convention: a Hijri date corresponds to the civil (midnight-to-midnight)
//! day that begins after its first sunset, and civil days are given as
//! integer Julian Day Numbers (the Julian Date at noon of that civil day).

pub mod calendar;
pub mod civil;
pub mod date;
pub mod newmoon;
pub mod observed;
pub mod tabular;
pub mod umalqura;

pub use calendar::{HijriCalendar, SupportedRange};
pub use civil::{CivilCalendar, CivilDate};
pub use date::{HijriDate, HijriMonth, Weekday};
pub use newmoon::{NewMoonRule, NewMoonRuleProvider};
pub use observed::{CalendarProvider, MonthStartProvider, ObservedCalendar, TableProvider};
pub use tabular::{Intercalation, TabularCalendar, TabularEpoch};
pub use umalqura::{TableCalendar, UMM_AL_QURA, UMM_AL_QURA_ICU};
