//! Hijri dates, month names and weekday names.

use core::fmt;

/// A date in some Hijri calendar.
///
/// The same `HijriDate` can denote different civil days in different
/// calendars; convert with a [`HijriCalendar`](crate::HijriCalendar).
/// Ordering is chronological within one calendar (year, then month, then day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HijriDate {
    /// Year AH (anno Hegirae).
    pub year: i32,
    /// Month, 1 (Muḥarram) to 12 (Dhū al-Ḥijjah).
    pub month: u8,
    /// Day of the month, 1 to 30 (31 in a few historical Umm al-Qura months).
    pub day: u8,
}

impl HijriDate {
    /// Creates a date without validating it against any calendar.
    pub const fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    /// The month as a [`HijriMonth`], or `None` if `month` is not 1 to 12.
    pub fn hijri_month(self) -> Option<HijriMonth> {
        HijriMonth::from_number(self.month)
    }
}

impl fmt::Display for HijriDate {
    /// Formats as `"1 Ramadan 1445 AH"` (ASCII month name).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.hijri_month() {
            Some(m) => write!(f, "{} {} {} AH", self.day, m.name_ascii(), self.year),
            None => write!(f, "{}/{}/{} AH", self.day, self.month, self.year),
        }
    }
}

/// The twelve Hijri months.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
#[allow(missing_docs)]
pub enum HijriMonth {
    Muharram = 1,
    Safar,
    RabiAlAwwal,
    RabiAlThani,
    JumadaAlUla,
    JumadaAlAkhirah,
    Rajab,
    Shaban,
    Ramadan,
    Shawwal,
    DhuAlQadah,
    DhuAlHijjah,
}

const MONTH_NAMES: [(&str, &str, &str); 12] = [
    ("Muḥarram", "Muharram", "محرم"),
    ("Ṣafar", "Safar", "صفر"),
    ("Rabīʿ al-Awwal", "Rabi al-Awwal", "ربيع الأول"),
    ("Rabīʿ al-Thānī", "Rabi al-Thani", "ربيع الآخر"),
    ("Jumādā al-Ūlā", "Jumada al-Ula", "جمادى الأولى"),
    ("Jumādā al-Ākhirah", "Jumada al-Akhirah", "جمادى الآخرة"),
    ("Rajab", "Rajab", "رجب"),
    ("Shaʿbān", "Shaban", "شعبان"),
    ("Ramaḍān", "Ramadan", "رمضان"),
    ("Shawwāl", "Shawwal", "شوال"),
    ("Dhū al-Qaʿdah", "Dhu al-Qadah", "ذو القعدة"),
    ("Dhū al-Ḥijjah", "Dhu al-Hijjah", "ذو الحجة"),
];

impl HijriMonth {
    /// All months in order.
    pub const ALL: [HijriMonth; 12] = [
        Self::Muharram,
        Self::Safar,
        Self::RabiAlAwwal,
        Self::RabiAlThani,
        Self::JumadaAlUla,
        Self::JumadaAlAkhirah,
        Self::Rajab,
        Self::Shaban,
        Self::Ramadan,
        Self::Shawwal,
        Self::DhuAlQadah,
        Self::DhuAlHijjah,
    ];

    /// The month numbered `n` (1 to 12).
    pub fn from_number(n: u8) -> Option<Self> {
        Self::ALL.get(usize::from(n).wrapping_sub(1)).copied()
    }

    /// Month number, 1 to 12.
    pub const fn number(self) -> u8 {
        self as u8
    }

    /// Transliterated name with diacritics, e.g. `"Ramaḍān"`.
    pub const fn name(self) -> &'static str {
        MONTH_NAMES[self as usize - 1].0
    }

    /// Plain-ASCII transliteration, e.g. `"Ramadan"`, for fonts lacking the
    /// Latin Extended Additional block.
    pub const fn name_ascii(self) -> &'static str {
        MONTH_NAMES[self as usize - 1].1
    }

    /// Name in Arabic script (Saudi usage: ربيع الآخر for month 4).
    pub const fn name_arabic(self) -> &'static str {
        MONTH_NAMES[self as usize - 1].2
    }
}

/// Day of the week.
///
/// Note that the Islamic day begins at sunset: the evening before a civil
/// Friday is already *laylat al-jumuʿah*. Weekdays here follow the civil day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum Weekday {
    Sunday = 0,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

const WEEKDAY_NAMES: [(&str, &str, &str, &str); 7] = [
    ("Sunday", "al-Aḥad", "al-Ahad", "الأحد"),
    ("Monday", "al-Ithnayn", "al-Ithnayn", "الاثنين"),
    ("Tuesday", "al-Thulāthāʾ", "al-Thulatha", "الثلاثاء"),
    ("Wednesday", "al-Arbiʿāʾ", "al-Arbia", "الأربعاء"),
    ("Thursday", "al-Khamīs", "al-Khamis", "الخميس"),
    ("Friday", "al-Jumuʿah", "al-Jumuah", "الجمعة"),
    ("Saturday", "al-Sabt", "al-Sabt", "السبت"),
];

impl Weekday {
    /// All weekdays, Sunday first.
    pub const ALL: [Weekday; 7] = [
        Self::Sunday,
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
    ];

    /// Weekday of civil day `jdn` (JDN 0 was a Monday).
    pub fn from_jdn(jdn: i64) -> Self {
        Self::ALL[(jdn + 1).rem_euclid(7) as usize]
    }

    /// Index with Sunday = 0 ... Saturday = 6.
    pub const fn index_from_sunday(self) -> u8 {
        self as u8
    }

    /// Days from `start` to `self` going forwards (0 to 6).
    pub const fn days_since(self, start: Weekday) -> u8 {
        (self as u8 + 7 - start as u8) % 7
    }

    /// English name.
    pub const fn name_english(self) -> &'static str {
        WEEKDAY_NAMES[self as usize].0
    }

    /// Arabic name transliterated with diacritics, e.g. `"al-Jumuʿah"`.
    pub const fn name_transliterated(self) -> &'static str {
        WEEKDAY_NAMES[self as usize].1
    }

    /// Arabic name in plain ASCII, e.g. `"al-Jumuah"`.
    pub const fn name_ascii(self) -> &'static str {
        WEEKDAY_NAMES[self as usize].2
    }

    /// Arabic name in Arabic script, e.g. `"الجمعة"`.
    pub const fn name_arabic(self) -> &'static str {
        WEEKDAY_NAMES[self as usize].3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays() {
        // 1 January 2000 was a Saturday; 16 July 622 (Julian) a Friday.
        assert_eq!(Weekday::from_jdn(2_451_545), Weekday::Saturday);
        assert_eq!(Weekday::from_jdn(1_948_440), Weekday::Friday);
        assert_eq!(Weekday::from_jdn(1_948_439), Weekday::Thursday);
        assert_eq!(Weekday::from_jdn(0), Weekday::Monday);
        assert_eq!(Weekday::from_jdn(-1), Weekday::Sunday);
        assert_eq!(Weekday::Friday.name_arabic(), "الجمعة");
        assert_eq!(Weekday::Sunday.days_since(Weekday::Saturday), 1);
        assert_eq!(Weekday::Saturday.days_since(Weekday::Sunday), 6);
    }

    #[test]
    fn months() {
        assert_eq!(HijriMonth::from_number(0), None);
        assert_eq!(HijriMonth::from_number(13), None);
        for (i, m) in HijriMonth::ALL.iter().enumerate() {
            assert_eq!(usize::from(m.number()), i + 1);
            assert_eq!(HijriMonth::from_number(m.number()), Some(*m));
            assert!(m.name_ascii().is_ascii());
            assert!(!m.name_arabic().is_ascii());
        }
        assert_eq!(HijriMonth::Ramadan.name(), "Ramaḍān");
        assert_eq!(HijriMonth::Ramadan.name_arabic(), "رمضان");
        assert_eq!(HijriDate::new(1445, 9, 1).to_string(), "1 Ramadan 1445 AH");
        assert!(HijriDate::new(1445, 9, 1) < HijriDate::new(1445, 10, 1));
    }
}
