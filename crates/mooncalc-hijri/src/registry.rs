//! The registry of built-in calendars, for a GUI calendar selector.
//!
//! Observation-based calendars need a location and criterion, so they are
//! built by the caller ([`ObservedCalendar`](crate::ObservedCalendar)) rather
//! than listed here.

use core::fmt;

use crate::HijriCalendar;
use crate::tabular::{
    TABULAR_I_ASTRONOMICAL, TABULAR_I_CIVIL, TABULAR_II_ASTRONOMICAL, TABULAR_II_CIVIL,
    TABULAR_III_ASTRONOMICAL, TABULAR_III_CIVIL, TABULAR_IV_ASTRONOMICAL, TABULAR_IV_CIVIL,
};
use crate::umalqura::{ATTRIBUTION_ICU, ATTRIBUTION_OFFICIAL, UMM_AL_QURA, UMM_AL_QURA_ICU};

/// Id of the calendar a GUI should select by default.
pub const DEFAULT_CALENDAR_ID: &str = "umm-al-qura";

/// The family a calendar belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CalendarKind {
    /// Arithmetical 30-year-cycle calendar.
    Tabular,
    /// Table-driven Umm al-Qura calendar.
    UmmAlQura,
}

/// A built-in calendar and its descriptive metadata.
pub struct CalendarInfo {
    /// Stable identifier (equals `calendar.id()`); safe to persist.
    pub id: &'static str,
    /// Display name (equals `calendar.name()`).
    pub name: &'static str,
    /// One-sentence description for a tooltip.
    pub description: &'static str,
    /// The calendar family.
    pub kind: CalendarKind,
    /// The matching CLDR/ICU calendar identifier, if there is one
    /// (`Intl.DateTimeFormat` calendar option).
    pub cldr_id: Option<&'static str>,
    /// Attribution text that must be shown with the calendar's data.
    pub attribution: Option<&'static str>,
    /// The calendar itself.
    pub calendar: &'static (dyn HijriCalendar + Sync),
}

impl fmt::Debug for CalendarInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CalendarInfo")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("cldr_id", &self.cldr_id)
            .finish_non_exhaustive()
    }
}

static CALENDARS: [CalendarInfo; 10] = [
    CalendarInfo {
        id: "umm-al-qura",
        name: "Umm al-Qura (Saudi Arabia)",
        description: "Official calendar of Saudi Arabia as published, 1343-1500 AH (1924-2077 CE).",
        kind: CalendarKind::UmmAlQura,
        cldr_id: None,
        attribution: Some(ATTRIBUTION_OFFICIAL),
        calendar: &UMM_AL_QURA,
    },
    CalendarInfo {
        id: "umm-al-qura-icu",
        name: "Umm al-Qura (ICU/CLDR table)",
        description: "Umm al-Qura as tabulated by ICU and web browsers, 1300-1600 AH; matches the \
                      official calendar only for 1420-1450 AH.",
        kind: CalendarKind::UmmAlQura,
        cldr_id: Some("islamic-umalqura"),
        attribution: Some(ATTRIBUTION_ICU),
        calendar: &UMM_AL_QURA_ICU,
    },
    CalendarInfo {
        id: "tabular-2-civil",
        name: "Tabular II (civil epoch)",
        description: "The most common arithmetical calendar (leap years 2, 5, 7, 10, 13, 16, 18, \
                      21, 24, 26, 29), epoch Friday 16 July 622.",
        kind: CalendarKind::Tabular,
        cldr_id: Some("islamic-civil"),
        attribution: None,
        calendar: &TABULAR_II_CIVIL,
    },
    CalendarInfo {
        id: "tabular-2-astronomical",
        name: "Tabular II (astronomical epoch)",
        description: "Type II arithmetical calendar with epoch Thursday 15 July 622, as used by \
                      Microsoft's HijriCalendar (\"Kuwaiti algorithm\").",
        kind: CalendarKind::Tabular,
        cldr_id: Some("islamic-tbla"),
        attribution: None,
        calendar: &TABULAR_II_ASTRONOMICAL,
    },
    CalendarInfo {
        id: "tabular-1-civil",
        name: "Tabular I (civil epoch)",
        description: "Arithmetical calendar of Kushyar ibn Labban and Ulugh Beg (leap years 2, 5, \
                      7, 10, 13, 15, 18, 21, 24, 26, 29), epoch Friday 16 July 622.",
        kind: CalendarKind::Tabular,
        cldr_id: None,
        attribution: None,
        calendar: &TABULAR_I_CIVIL,
    },
    CalendarInfo {
        id: "tabular-1-astronomical",
        name: "Tabular I (astronomical epoch)",
        description: "Type I arithmetical calendar with epoch Thursday 15 July 622.",
        kind: CalendarKind::Tabular,
        cldr_id: None,
        attribution: None,
        calendar: &TABULAR_I_ASTRONOMICAL,
    },
    CalendarInfo {
        id: "tabular-3-civil",
        name: "Tabular III, Fatimid (civil epoch)",
        description: "Fatimid (Ismaili, Bohra) arithmetical calendar (leap years 2, 5, 8, 10, 13, \
                      16, 19, 21, 24, 27, 29), epoch Friday 16 July 622.",
        kind: CalendarKind::Tabular,
        cldr_id: None,
        attribution: None,
        calendar: &TABULAR_III_CIVIL,
    },
    CalendarInfo {
        id: "tabular-3-astronomical",
        name: "Tabular III, Fatimid (astronomical epoch)",
        description: "Fatimid arithmetical calendar with epoch Thursday 15 July 622.",
        kind: CalendarKind::Tabular,
        cldr_id: None,
        attribution: None,
        calendar: &TABULAR_III_ASTRONOMICAL,
    },
    CalendarInfo {
        id: "tabular-4-civil",
        name: "Tabular IV (civil epoch)",
        description: "Arithmetical calendar of Habash al-Hasib and al-Biruni (leap years 2, 5, 8, \
                      11, 13, 16, 19, 21, 24, 27, 30), epoch Friday 16 July 622.",
        kind: CalendarKind::Tabular,
        cldr_id: None,
        attribution: None,
        calendar: &TABULAR_IV_CIVIL,
    },
    CalendarInfo {
        id: "tabular-4-astronomical",
        name: "Tabular IV (astronomical epoch)",
        description: "Type IV arithmetical calendar with epoch Thursday 15 July 622.",
        kind: CalendarKind::Tabular,
        cldr_id: None,
        attribution: None,
        calendar: &TABULAR_IV_ASTRONOMICAL,
    },
];

/// All built-in calendars, the default first.
pub fn calendars() -> &'static [CalendarInfo] {
    &CALENDARS
}

/// Looks up a built-in calendar by its stable id.
pub fn calendar_by_id(id: &str) -> Option<&'static (dyn HijriCalendar + Sync)> {
    info_by_id(id).map(|info| info.calendar)
}

/// Looks up a built-in calendar's metadata by its stable id.
pub fn info_by_id(id: &str) -> Option<&'static CalendarInfo> {
    CALENDARS.iter().find(|info| info.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_is_consistent() {
        let mut ids = HashSet::new();
        for info in calendars() {
            assert!(ids.insert(info.id), "duplicate id {}", info.id);
            assert_eq!(info.calendar.id(), info.id);
            assert_eq!(info.calendar.name(), info.name);
            assert!(info.description.is_ascii());
            assert!(info.calendar.supported_jdn_range().is_some());
        }
        assert_eq!(calendars()[0].id, DEFAULT_CALENDAR_ID);
        assert!(calendar_by_id(DEFAULT_CALENDAR_ID).is_some());
        assert!(calendar_by_id("nonexistent").is_none());
        assert_eq!(
            info_by_id("tabular-2-civil").unwrap().cldr_id,
            Some("islamic-civil")
        );
    }
}
