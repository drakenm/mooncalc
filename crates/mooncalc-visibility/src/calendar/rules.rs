//! Calendars from astronomical rules (no crescent sighting): the Umm
//! al-Qura rules and conjunction rules, computed with a real ephemeris.

use std::sync::Arc;

use mooncalc_core::events::phases::nearest_new_moon;
use mooncalc_core::events::{Body, CalendarDate, DirectSource, Horizon, day_events};
use mooncalc_core::time::Instant;
use mooncalc_core::{Ephemeris, GeoLocation};
use mooncalc_hijri::newmoon::{ConjunctionBeforeLocalTime, ConjunctionBeforeSunset};
use mooncalc_hijri::{HijriCalendar, NewMoonRuleProvider, TwentyNinthDayProvider, UMM_AL_QURA};

use super::sighting::Conjunctions;
use super::{BoxedConjunction, BoxedTest, DynProvider};
use crate::circumstances::{Outcome, Request, Settings, Sighting, compute_with};
use crate::criteria::EvaluationTime;

/// The Kaʿba, Mecca, whose position the Umm al-Qura rules use (van Gent,
/// "The Umm al-Qura Calendar of Saudi Arabia", introduction).
pub const MECCA: GeoLocation = GeoLocation::new(21.4225, 39.8262, 0.0);

/// Saudi civil time, hours east of UT.
pub const MECCA_UTC_OFFSET_HOURS: f64 = 3.0;

/// The evening test of the Umm al-Qura rules, made on the 29th day of each
/// month at Mecca (R.H. van Gent, *The Umm al-Qura Calendar of Saudi
/// Arabia*, "Rules", <https://webspace.science.uu.nl/~gent0113/islam/ummalqura_rules.htm>):
///
/// * **1420–1422 AH** (from 17 April 1999): the next day is the first of
///   the new month "if the moonset is after sunset at Mecca", otherwise the
///   month has 30 days.
/// * **From 1423 AH** (15 March 2002): if on the 29th day "the geocentric
///   conjunction occurs before sunset" and "the moon sets after the sun",
///   the next day is the first of the new month; otherwise 30 days.
///
/// Before 1420 AH the official calendar followed other rules (1392–1419: a
/// conjunction-time rule); this test applies the 1423 rule there as an
/// extrapolation.
///
/// Sunset and moonset use the standard horizon (upper limb, 34′
/// refraction, level horizon) at the Kaʿba; the rule's source gives no
/// horizon convention. Days are Saudi civil days (UT+3).
pub struct UmmAlQuraRule {
    eph: Arc<dyn Ephemeris>,
    conjunctions: Conjunctions,
    /// 1 Muḥarram 1420 and 1423 (JDN), when the rules changed.
    start_1420: i64,
    start_1423: i64,
}

impl core::fmt::Debug for UmmAlQuraRule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UmmAlQuraRule").finish_non_exhaustive()
    }
}

impl UmmAlQuraRule {
    pub fn new(eph: Arc<dyn Ephemeris>) -> Self {
        Self {
            conjunctions: Conjunctions::new(eph.clone()),
            eph,
            start_1420: UMM_AL_QURA.month_start(1420, 1).expect("in table"),
            start_1423: UMM_AL_QURA.month_start(1423, 1).expect("in table"),
        }
    }

    /// The circumstances at sunset at Mecca on Saudi civil day `jdn`.
    fn sunset_at_mecca(&self, jdn: i64) -> Outcome {
        let date = CalendarDate::from_jd(jdn as f64);
        let noon = date.jd0() + 0.5 - MECCA_UTC_OFFSET_HOURS / 24.0;
        let req = Request {
            location: MECCA,
            noon: Instant::from_jd_ut(noon),
            sighting: Sighting::Evening,
            evaluation: EvaluationTime::Sunset,
        };
        let moons = self.conjunctions.around(noon);
        compute_with(
            &DirectSource::new(&*self.eph),
            &moons,
            &req,
            &Settings::default(),
        )
    }

    /// Whether the month ends on the evening of Saudi civil day `jdn` (the
    /// 29th of the month), so that the next month starts on `jdn + 1`.
    pub fn ends_month(&self, jdn: i64) -> bool {
        let o = self.sunset_at_mecca(jdn);
        let moon_sets_after_sun = matches!(
            o,
            Outcome::Crescent(_) | Outcome::BeforeConjunction(_) | Outcome::MoonAlwaysUp { .. }
        );
        if (self.start_1420..self.start_1423).contains(&jdn) {
            return moon_sets_after_sun;
        }
        // At sunset the age is sunset − nearest conjunction.
        let conjunction_before_sunset = o.geometry().is_some_and(|g| g.age_hours > 0.0);
        moon_sets_after_sun && conjunction_before_sunset
    }
}

/// The Umm al-Qura rules as a month-start provider (see [`UmmAlQuraRule`]).
pub fn umm_al_qura_rule_provider(
    eph: Arc<dyn Ephemeris>,
) -> TwentyNinthDayProvider<BoxedConjunction, BoxedTest> {
    let rule = Arc::new(UmmAlQuraRule::new(eph));
    let r = rule.clone();
    TwentyNinthDayProvider::new(
        Box::new(move |jd| Some(r.conjunctions.nearest(jd).jd_ut)),
        Box::new(move |jdn| rule.ends_month(jdn)),
    )
}

/// Sunset (JD UT) at Mecca on Saudi civil day `jdn`.
fn mecca_sunset(eph: &dyn Ephemeris, jdn: i64) -> Option<f64> {
    let (t0, t1) = CalendarDate::from_jd(jdn as f64).local_day(MECCA_UTC_OFFSET_HOURS);
    let src = DirectSource::new(eph);
    day_events(&src, Body::Sun, &MECCA, Horizon::STANDARD, t0, t1)
        .set
        .map(|t| t.jd_ut)
}

/// Rule-based calendars offered by the GUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuleCalendar {
    /// The Umm al-Qura rules ([`UmmAlQuraRule`]).
    UmmAlQuraRule,
    /// The month begins the day after the conjunction if the conjunction is
    /// before sunset at Mecca on its Saudi civil day, otherwise a day later.
    ConjunctionBeforeSunsetMecca,
    /// The Fiqh Council of North America's rule (adopted by ISNA): the month
    /// begins the day after the conjunction if the conjunction is before
    /// 12:00 UT, otherwise a day later.
    ConjunctionBeforeNoonUt,
}

impl RuleCalendar {
    pub const ALL: [RuleCalendar; 3] = [
        RuleCalendar::UmmAlQuraRule,
        RuleCalendar::ConjunctionBeforeSunsetMecca,
        RuleCalendar::ConjunctionBeforeNoonUt,
    ];

    /// Stable identifier, safe to persist.
    pub fn id(self) -> &'static str {
        match self {
            RuleCalendar::UmmAlQuraRule => "rule-umm-al-qura",
            RuleCalendar::ConjunctionBeforeSunsetMecca => "rule-conjunction-before-sunset-mecca",
            RuleCalendar::ConjunctionBeforeNoonUt => "rule-conjunction-before-noon-ut",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            RuleCalendar::UmmAlQuraRule => "Umm al-Qura rule (computed)",
            RuleCalendar::ConjunctionBeforeSunsetMecca => "Conjunction before sunset at Mecca",
            RuleCalendar::ConjunctionBeforeNoonUt => "Conjunction before 12:00 UT (FCNA/ISNA)",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            RuleCalendar::UmmAlQuraRule => {
                "On the 29th day at Mecca: a new month starts the next day if the conjunction \
                 is before sunset and the Moon sets after the Sun (1420–1422 AH: moonset after \
                 sunset only); otherwise the month has 30 days. Computed with the selected \
                 ephemeris."
            }
            RuleCalendar::ConjunctionBeforeSunsetMecca => {
                "The month starts the day after the conjunction if the conjunction is before \
                 sunset at Mecca on its Saudi civil day, otherwise two days after."
            }
            RuleCalendar::ConjunctionBeforeNoonUt => {
                "The month starts the day after the conjunction if the conjunction is before \
                 12:00 UT, otherwise two days after (Fiqh Council of North America, 2006)."
            }
        }
    }

    /// Bibliographic source of the rule.
    pub fn reference(self) -> &'static str {
        match self {
            RuleCalendar::UmmAlQuraRule | RuleCalendar::ConjunctionBeforeSunsetMecca => {
                "R.H. van Gent, The Umm al-Qura Calendar of Saudi Arabia, Utrecht University \
                 (webspace.science.uu.nl/~gent0113/islam/ummalqura_rules.htm)"
            }
            RuleCalendar::ConjunctionBeforeNoonUt => {
                "Fiqh Council of North America, decision on the Islamic lunar calendar (2006), \
                 adopted by the Islamic Society of North America"
            }
        }
    }

    /// The month-start provider for this rule, computed with `eph`.
    pub fn provider(self, eph: Arc<dyn Ephemeris>) -> DynProvider {
        let conjunction_near = {
            let eph = eph.clone();
            move |jd: f64| Some(nearest_new_moon(&*eph, Instant::from_jd_ut(jd)).jd_ut)
        };
        match self {
            RuleCalendar::UmmAlQuraRule => Box::new(umm_al_qura_rule_provider(eph)),
            RuleCalendar::ConjunctionBeforeSunsetMecca => Box::new(NewMoonRuleProvider::new(
                conjunction_near,
                ConjunctionBeforeSunset {
                    utc_offset_hours: MECCA_UTC_OFFSET_HOURS,
                    sunset: move |jdn: i64| mecca_sunset(&*eph, jdn),
                },
            )),
            RuleCalendar::ConjunctionBeforeNoonUt => Box::new(NewMoonRuleProvider::new(
                conjunction_near,
                ConjunctionBeforeLocalTime {
                    utc_offset_hours: 0.0,
                    cutoff_hours: 12.0,
                },
            )),
        }
    }
}
