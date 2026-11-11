//! Ephemeris tables and event lists for the Sun and Moon: the services
//! behind a frontend's Ephemeris tab.
//!
//! | Operation | Source |
//! | --- | --- |
//! | [`ephemeris_table`] | `mooncalc_core::tables` (Meeus ch. 13, 16, 40, 48, 53) |
//! | [`rise_set_month`] | `mooncalc_core::events` (`day_events`, `twilight`): upper limb with 34′ refraction; Sun's centre at −6°, −12°, −18° |
//! | [`lunar_phases`] | `mooncalc_core::events::phases` (Meeus ch. 49, refined against the ephemeris) |
//! | [`apsides_nodes`] | `mooncalc_core::{apsides, nodes}` (Meeus ch. 50–52, refined) |
//! | [`eclipses`] | `mooncalc_core::eclipses` (Meeus ch. 54; Besselian elements and Danjon's shadow; local circumstances) |
//! | [`seasons`] | `mooncalc_core::seasons` (Meeus ch. 27 refined; equation of time, eq. 28.1) |
//!
//! # Threads
//!
//! The crate is portable and single-threaded. The two long operations are
//! split so callers can spread them over threads: a table's rows
//! ([`check_table`], then [`table_rows`] per chunk of [`table_row_chunks`]),
//! and eclipses ([`eclipse_candidates`], then [`eclipse_details`] per
//! eclipse). [`ephemeris_table`] and [`eclipses`] do everything in one call.

pub mod dto;

pub use dto::*;

use std::collections::BTreeMap;
use std::ops::Range;

use mooncalc_core::angle::{ARCMIN, DEG};
use mooncalc_core::apsides::{ApsisKind, apsides_between};
use mooncalc_core::eclipses::{
    Eclipse, EclipseFilter, LocalEclipseKind, LunarEclipseKind, SolarEclipseKind,
    eclipses_in_years, local_solar_eclipse, refine_lunar_eclipse, refine_solar_eclipse,
};
use mooncalc_core::events::phases::{
    Phase, SYNODIC_MONTH_DAYS, meeus_phase_jde, phase_near_tt, phases_between,
};
use mooncalc_core::events::riseset::Circumpolar;
use mooncalc_core::events::{
    ApparentTable, Body, CalendarDate, DirectSource, Horizon, Observer, PositionSource, Twilight,
    day_events, twilight,
};
use mooncalc_core::nodes::{
    DeclinationExtreme, NodeKind, max_declinations_between, node_passages_between,
};
use mooncalc_core::refraction::{Atmosphere, apparent_altitude};
use mooncalc_core::seasons::{Season, equation_of_time, seasons_of_year};
use mooncalc_core::tables::{BodyRow, EphemerisRow, TableOptions};
use mooncalc_core::time::Instant;
use mooncalc_core::topocentric::moon_semidiameter;
use mooncalc_core::{Ephemeris, GeoLocation};

use crate::dto::{ApiError, CivilDate, Location, UtDateTime};
use crate::ephemeris::check_covers;
use crate::validate;

/// Most rows one table request may ask for.
pub const MAX_TABLE_ROWS: u32 = 10_000;
/// Rows per work unit of [`table_row_chunks`].
pub const DEFAULT_TABLE_CHUNK: usize = 250;
/// Most years one eclipse request may span.
pub const MAX_ECLIPSE_YEARS: i32 = 200;

/// Minutes of time per radian of hour angle.
const MIN_PER_RAD: f64 = 720.0 / std::f64::consts::PI;

/// Epoch of Meeus' lunation numbers (k = 0: new Moon of 2000 January 6).
const LUNATION_EPOCH_JDE: f64 = 2_451_550.097_66;

fn deg(rad: f64) -> f64 {
    rad / DEG
}

/// Degrees in `[0, 360)`.
fn deg360(rad: f64) -> f64 {
    (rad / DEG).rem_euclid(360.0)
}

fn ut(t: Instant) -> UtDateTime {
    UtDateTime::from(t)
}

fn ut_tt(jd_tt: f64) -> UtDateTime {
    ut(Instant::from_jd_tt(jd_tt))
}

fn check_time(t: &UtDateTime) -> Result<(), ApiError> {
    validate::date(t.date())?;
    if t.hour > 23 || t.minute > 59 || !(0.0..60.0).contains(&t.second) {
        return Err(ApiError::invalid_params("time of day out of range"));
    }
    Ok(())
}

/// UT bounds of local `year` (midnight to midnight in the zone).
fn year_bounds(year: i32, tz: f64) -> (Instant, Instant) {
    let start = Instant::from_calendar(year, 1, 1, 0, 0, 0.0).add_days(-tz / 24.0);
    let end = Instant::from_calendar(year + 1, 1, 1, 0, 0, 0.0).add_days(-tz / 24.0);
    (start, end)
}

fn check_year(eph: &dyn Ephemeris, p: &YearParams) -> Result<(Instant, Instant), ApiError> {
    if !(-4000..=9999).contains(&p.year) {
        return Err(ApiError::invalid_params(
            "year must be between -4000 and 9999",
        ));
    }
    validate::tz_offset(p.tz_offset_hours)?;
    let (t0, t1) = year_bounds(p.year, p.tz_offset_hours);
    check_covers(eph, t0.jd_tt() - 40.0, t1.jd_tt() + 40.0)?;
    Ok((t0, t1))
}

// ---------------------------------------------------------------------------
// Ephemeris table
// ---------------------------------------------------------------------------

fn body_dto(b: &BodyRow) -> BodyRowDto {
    BodyRowDto {
        ra_deg: deg360(b.equatorial.ra),
        dec_deg: deg(b.equatorial.dec),
        lon_deg: deg360(b.ecliptic.lon),
        lat_deg: deg(b.ecliptic.lat),
        distance_km: b.equatorial.distance_km,
        semidiameter_arcmin: b.semidiameter / ARCMIN,
        parallax_arcmin: b.horizontal_parallax / ARCMIN,
        topocentric: b.topocentric.map(|t| TopocentricDto {
            alt_deg: deg(t.apparent_altitude),
            alt_airless_deg: deg(t.horizontal.alt),
            az_deg: deg360(t.horizontal.az),
            ra_deg: deg360(t.equatorial.ra),
            dec_deg: deg(t.equatorial.dec),
            distance_km: t.equatorial.distance_km,
            semidiameter_arcmin: t.semidiameter / ARCMIN,
        }),
    }
}

/// The wire form of a core table row.
pub fn row_dto(r: &EphemerisRow) -> EphemerisRowDto {
    let p = &r.physical;
    EphemerisRowDto {
        time: ut(r.instant),
        jd_ut: r.instant.jd_ut,
        jd_tt: r.jd_tt,
        sun: body_dto(&r.sun),
        moon: body_dto(&r.moon),
        elongation_deg: deg(r.illumination.elongation),
        phase_angle_deg: deg(r.illumination.phase_angle),
        illuminated_fraction: r.illumination.fraction,
        bright_limb_deg: deg360(r.illumination.bright_limb_angle),
        topocentric_elongation_deg: r.topocentric_illumination.map(|i| deg(i.elongation)),
        moon_age_days: r.moon_age_days,
        conjunction: ut_tt(r.conjunction_jd_tt),
        libration: LibrationDto {
            lon_deg: deg(p.libration.lon),
            lat_deg: deg(p.libration.lat),
            axis_position_angle_deg: deg360(p.libration.position_angle),
            colongitude_deg: deg360(p.colongitude),
            sun_lat_deg: deg(p.sun_lat),
        },
    }
}

/// Validate a table request; returns the row count.
pub fn check_table(eph: &dyn Ephemeris, p: &EphemerisTableParams) -> Result<usize, ApiError> {
    check_time(&p.start)?;
    if !(0.1..=525_600.0).contains(&p.step_minutes) {
        return Err(ApiError::invalid_params(
            "stepMinutes must be between 0.1 and 525600",
        ));
    }
    if !(1..=MAX_TABLE_ROWS).contains(&p.count) {
        return Err(ApiError::invalid_params(format!(
            "count must be between 1 and {MAX_TABLE_ROWS}"
        )));
    }
    if let Some(l) = p.location {
        validate::location(l)?;
    }
    let start = p.start.to_instant();
    let end = start.add_days(p.step_minutes / 1440.0 * f64::from(p.count - 1));
    check_covers(eph, start.jd_tt() - 31.0, end.jd_tt() + 1.0)?;
    Ok(p.count as usize)
}

/// Row ranges of `count` rows in chunks of `chunk`.
pub fn table_row_chunks(count: usize, chunk: usize) -> Vec<Range<usize>> {
    let chunk = chunk.max(1);
    (0..count.div_ceil(chunk))
        .map(|i| i * chunk..((i + 1) * chunk).min(count))
        .collect()
}

/// Rows `range` of a checked table ([`check_table`]).
pub fn table_rows(
    eph: &dyn Ephemeris,
    p: &EphemerisTableParams,
    range: Range<usize>,
) -> Vec<EphemerisRowDto> {
    let step = p.step_minutes / 1440.0;
    let start = p.start.to_instant().add_days(step * range.start as f64);
    let opts = TableOptions {
        location: p.location.map(GeoLocation::from),
        ..TableOptions::default()
    };
    mooncalc_core::tables::ephemeris_table(eph, start, step, range.len(), &opts)
        .iter()
        .map(row_dto)
        .collect()
}

/// A whole table in one call.
pub fn ephemeris_table(
    eph: &dyn Ephemeris,
    p: &EphemerisTableParams,
) -> Result<EphemerisTable, ApiError> {
    let n = check_table(eph, p)?;
    Ok(EphemerisTable {
        rows: table_rows(eph, p, 0..n),
        ephemeris: eph.name().into(),
    })
}

// ---------------------------------------------------------------------------
// Rise and set
// ---------------------------------------------------------------------------

fn circumpolar_code(c: Circumpolar) -> &'static str {
    match c {
        Circumpolar::AlwaysUp => "always-up",
        Circumpolar::AlwaysDown => "always-down",
    }
}

fn body_day<S: PositionSource + ?Sized>(
    src: &S,
    obs: &Observer,
    loc: &GeoLocation,
    body: Body,
    t0: Instant,
    t1: Instant,
) -> BodyDayDto {
    let atm = Atmosphere::STANDARD;
    let angle = |t: Instant, azimuth: bool| -> f64 {
        let f = obs.frame_at(src, t.jd_ut);
        let v = f.topocentric(&src.position(body, t.jd_ut));
        if azimuth {
            deg360(f.azimuth(&v))
        } else {
            deg(apparent_altitude(f.altitude(&v), &atm))
        }
    };
    let event = |t: Instant, azimuth: bool| AngledEventDto {
        time: ut(t),
        angle_deg: angle(t, azimuth),
    };
    let d = day_events(src, body, loc, Horizon::STANDARD, t0, t1);
    BodyDayDto {
        rise: d.rise.map(|t| event(t, true)),
        transit: d.upper_transit.map(|t| event(t, false)),
        set: d.set.map(|t| event(t, true)),
        circumpolar: d.circumpolar.map(|c| circumpolar_code(c).into()),
    }
}

fn twilight_dto(pair: (Option<Instant>, Option<Instant>)) -> TwilightDto {
    TwilightDto {
        dawn: pair.0.map(ut),
        dusk: pair.1.map(ut),
    }
}

/// A month of daily Sun and Moon rise, transit and set (with azimuths and
/// transit altitudes) and twilight, for one place, grouped by local day.
pub fn rise_set_month(eph: &dyn Ephemeris, p: &RiseSetParams) -> Result<RiseSetMonth, ApiError> {
    validate::date(CivilDate {
        year: p.year,
        month: p.month,
        day: 1,
    })?;
    validate::location(p.location)?;
    validate::tz_offset(p.tz_offset_hours)?;
    let first = CalendarDate::new(p.year, p.month, 1);
    let dates: Vec<CalendarDate> = (0..31)
        .map(|i| first.add_days(i))
        .take_while(|d| d.month == p.month)
        .collect();
    let (start, _) = first.local_day(p.tz_offset_hours);
    let end = start.add_days(dates.len() as f64);
    check_covers(eph, start.jd_tt() - 2.0, end.jd_tt() + 2.0)?;
    let table = ApparentTable::with_default_step(eph, start.add_days(-1.0), end.add_days(1.0));
    let loc = GeoLocation::from(p.location);
    let obs = Observer::new(&loc);
    let days = dates
        .into_iter()
        .map(|date| {
            let (t0, t1) = date.local_day(p.tz_offset_hours);
            RiseSetDay {
                date: date.into(),
                sun: body_day(&table, &obs, &loc, Body::Sun, t0, t1),
                moon: body_day(&table, &obs, &loc, Body::Moon, t0, t1),
                civil: twilight_dto(twilight(&table, &loc, Twilight::Civil, t0, t1)),
                nautical: twilight_dto(twilight(&table, &loc, Twilight::Nautical, t0, t1)),
                astronomical: twilight_dto(twilight(&table, &loc, Twilight::Astronomical, t0, t1)),
            }
        })
        .collect();
    Ok(RiseSetMonth {
        year: p.year,
        month: p.month,
        tz_offset_hours: p.tz_offset_hours,
        location: p.location,
        days,
        ephemeris: eph.name().into(),
    })
}

// ---------------------------------------------------------------------------
// Phases, apsides and nodes, seasons
// ---------------------------------------------------------------------------

/// The principal phases of the local year, grouped by lunation, with the
/// Moon's age at each and each lunation's length.
pub fn lunar_phases(eph: &dyn Ephemeris, p: &YearParams) -> Result<LunarPhasesYear, ApiError> {
    let (t0, t1) = check_year(eph, p)?;
    let mut by_k: BTreeMap<i64, [Option<Instant>; 4]> = BTreeMap::new();
    for (phase, t) in phases_between(eph, t0, t1) {
        let i = Phase::ALL.iter().position(|q| *q == phase).unwrap_or(0);
        let k = ((t.jd_tt() - LUNATION_EPOCH_JDE) / SYNODIC_MONTH_DAYS - phase.fraction()).round()
            as i64;
        by_k.entry(k).or_default()[i] = Some(t);
    }
    let new_moon = |k: i64| phase_near_tt(eph, Phase::New, meeus_phase_jde(k as f64));
    let lunations = by_k
        .into_iter()
        .map(|(k, phases)| {
            let start = new_moon(k);
            let start_ut = Instant::from_jd_tt(start);
            LunationDto {
                k,
                new_moon: ut(start_ut),
                length_days: new_moon(k + 1) - start,
                phases: phases.map(|t| t.map(ut)),
                ages_days: phases.map(|t| t.map(|t| t.jd_ut - start_ut.jd_ut)),
            }
        })
        .collect();
    Ok(LunarPhasesYear {
        year: p.year,
        lunations,
        ephemeris: eph.name().into(),
    })
}

/// Perigees and apogees, node passages and declination extremes of the
/// local year.
pub fn apsides_nodes(eph: &dyn Ephemeris, p: &YearParams) -> Result<ApsidesNodesYear, ApiError> {
    let (t0, t1) = check_year(eph, p)?;
    let (a, b) = (t0.jd_tt(), t1.jd_tt());
    let apsides = apsides_between(Some(eph), a, b)
        .into_iter()
        .map(|x| ApsisDto {
            kind: match x.kind {
                ApsisKind::Perigee => "perigee",
                ApsisKind::Apogee => "apogee",
            }
            .into(),
            time: ut_tt(x.jd_tt),
            distance_km: x.distance_km,
            parallax_arcsec: deg(x.parallax) * 3600.0,
            diameter_arcmin: 2.0 * moon_semidiameter(x.distance_km) / ARCMIN,
        })
        .collect();
    let nodes = node_passages_between(Some(eph), a, b)
        .into_iter()
        .map(|n| NodeDto {
            kind: match n.kind {
                NodeKind::Ascending => "ascending",
                NodeKind::Descending => "descending",
            }
            .into(),
            time: ut_tt(n.jd_tt),
        })
        .collect();
    let mut decl = max_declinations_between(Some(eph), a, b);
    decl.sort_by(|x, y| x.jd_tt.total_cmp(&y.jd_tt));
    let declinations = decl
        .into_iter()
        .map(|d| DeclinationDto {
            kind: match d.kind {
                DeclinationExtreme::North => "north",
                DeclinationExtreme::South => "south",
            }
            .into(),
            time: ut_tt(d.jd_tt),
            dec_deg: deg(d.dec),
        })
        .collect();
    Ok(ApsidesNodesYear {
        year: p.year,
        apsides,
        nodes,
        declinations,
        ephemeris: eph.name().into(),
    })
}

fn season_code(s: Season) -> (&'static str, &'static str) {
    match s {
        Season::MarchEquinox => ("march-equinox", "March equinox"),
        Season::JuneSolstice => ("june-solstice", "June solstice"),
        Season::SeptemberEquinox => ("september-equinox", "September equinox"),
        Season::DecemberSolstice => ("december-solstice", "December solstice"),
    }
}

/// Equinoxes and solstices of the year, and the equation of time at local
/// noon of every day.
pub fn seasons(eph: &dyn Ephemeris, p: &YearParams) -> Result<SeasonsYear, ApiError> {
    let (t0, t1) = check_year(eph, p)?;
    let eot = |t: Instant| equation_of_time(eph, t.jd_tt()) * MIN_PER_RAD;
    let jd = seasons_of_year(eph, p.year);
    let seasons = Season::ALL
        .into_iter()
        .zip(jd)
        .map(|(s, jd_tt)| {
            let t = Instant::from_jd_tt(jd_tt);
            let (kind, name) = season_code(s);
            SeasonDto {
                kind: kind.into(),
                name: name.into(),
                time: ut(t),
                equation_of_time_min: eot(t),
            }
        })
        .collect();
    let days = (t1.jd_ut - t0.jd_ut).round() as i32;
    let jan1 = CalendarDate::new(p.year, 1, 1);
    let equation_of_time = (0..days)
        .map(|d| EquationOfTimePoint {
            date: jan1.add_days(d).into(),
            minutes: eot(t0.add_days(f64::from(d) + 0.5)),
        })
        .collect();
    Ok(SeasonsYear {
        year: p.year,
        seasons,
        equation_of_time,
        ephemeris: eph.name().into(),
    })
}

// ---------------------------------------------------------------------------
// Eclipses
// ---------------------------------------------------------------------------

/// The eclipses of a request (Meeus ch. 54, cheap), in time order, for
/// [`eclipse_details`].
pub fn eclipse_candidates(
    eph: &dyn Ephemeris,
    p: &EclipsesParams,
) -> Result<Vec<Eclipse>, ApiError> {
    if p.last_year < p.first_year {
        return Err(ApiError::invalid_params("lastYear is before firstYear"));
    }
    if p.last_year - p.first_year >= MAX_ECLIPSE_YEARS {
        return Err(ApiError::invalid_params(format!(
            "at most {MAX_ECLIPSE_YEARS} years at a time"
        )));
    }
    if !(-4000..=9999).contains(&p.first_year) || !(-4000..=9999).contains(&p.last_year) {
        return Err(ApiError::invalid_params(
            "years must be between -4000 and 9999",
        ));
    }
    if let Some(l) = p.location {
        validate::location(l)?;
    }
    let (t0, _) = year_bounds(p.first_year, 0.0);
    let (_, t1) = year_bounds(p.last_year, 0.0);
    check_covers(eph, t0.jd_tt() - 2.0, t1.jd_tt() + 2.0)?;
    let filter = match p.filter {
        EclipseKindFilter::Any => EclipseFilter::Any,
        EclipseKindFilter::Solar => EclipseFilter::Solar,
        EclipseKindFilter::Lunar => EclipseFilter::Lunar,
    };
    Ok(eclipses_in_years(p.first_year, p.last_year, filter))
}

fn solar_kind(k: SolarEclipseKind) -> &'static str {
    match k {
        SolarEclipseKind::Partial => "partial",
        SolarEclipseKind::Annular => "annular",
        SolarEclipseKind::Total => "total",
        SolarEclipseKind::Hybrid => "hybrid",
    }
}

fn lunar_kind(k: LunarEclipseKind) -> &'static str {
    match k {
        LunarEclipseKind::Penumbral => "penumbral",
        LunarEclipseKind::Partial => "partial",
        LunarEclipseKind::Total => "total",
    }
}

fn local_kind(k: LocalEclipseKind) -> &'static str {
    match k {
        LocalEclipseKind::Partial => "partial",
        LocalEclipseKind::Annular => "annular",
        LocalEclipseKind::Total => "total",
    }
}

/// Sun's centre below this airless altitude counts as set (refraction plus
/// semi-diameter).
const SUN_SET_ALT_DEG: f64 = -0.833;

/// Refine one eclipse against the ephemeris and add local circumstances
/// at `location`.
pub fn eclipse_details(
    eph: &dyn Ephemeris,
    eclipse: &Eclipse,
    location: Option<Location>,
) -> EclipseDto {
    let loc = location.map(GeoLocation::from);
    match eclipse {
        Eclipse::Solar(s) => {
            let d = refine_solar_eclipse(eph, s);
            let contact = |c: &mooncalc_core::eclipses::EclipseContact| SolarContactDto {
                time: ut_tt(c.jd_tt),
                sun_alt_deg: deg(c.sun.alt),
                sun_az_deg: deg360(c.sun.az),
            };
            let local = loc
                .as_ref()
                .and_then(|l| local_solar_eclipse(eph, d.jd_tt, l))
                .map(|l| {
                    let all = [Some(l.c1), l.c2, Some(l.max), l.c3, Some(l.c4)];
                    LocalSolarDto {
                        kind: local_kind(l.kind).into(),
                        c1: contact(&l.c1),
                        c2: l.c2.as_ref().map(contact),
                        max: contact(&l.max),
                        c3: l.c3.as_ref().map(contact),
                        c4: contact(&l.c4),
                        magnitude: l.magnitude,
                        obscuration: l.obscuration,
                        moon_sun_ratio: l.moon_sun_ratio,
                        duration_min: (l.c4.jd_tt - l.c1.jd_tt) * 1440.0,
                        central_duration_min: l
                            .c2
                            .zip(l.c3)
                            .map(|(a, b)| (b.jd_tt - a.jd_tt) * 1440.0),
                        visible: all
                            .iter()
                            .flatten()
                            .any(|c| deg(c.sun.alt) > SUN_SET_ALT_DEG),
                    }
                });
            let kind = solar_kind(d.kind);
            EclipseDto {
                body: "solar".into(),
                kind: kind.into(),
                label: format!("Solar {kind}"),
                lunation: s.lunation,
                greatest: ut_tt(d.jd_tt),
                gamma: d.gamma,
                magnitude: d.magnitude,
                solar: Some(SolarEclipseDto {
                    central: d.central,
                    greatest_lat_deg: d.lat_deg,
                    greatest_lon_deg: d.lon_deg,
                    sun_alt_deg: deg(d.sun_alt),
                    local,
                }),
                lunar: None,
            }
        }
        Eclipse::Lunar(m) => {
            let d = refine_lunar_eclipse(eph, m);
            let src = DirectSource::new(eph);
            let obs = loc.as_ref().map(Observer::new);
            let contacts: Vec<LunarContactDto> = [
                ("P1", "Penumbral eclipse begins", d.p1),
                ("U1", "Partial eclipse begins", d.u1),
                ("U2", "Total eclipse begins", d.u2),
                ("max", "Greatest eclipse", Some(d.jd_tt)),
                ("U3", "Total eclipse ends", d.u3),
                ("U4", "Partial eclipse ends", d.u4),
                ("P4", "Penumbral eclipse ends", d.p4),
            ]
            .into_iter()
            .filter_map(|(code, label, jd)| {
                let time = Instant::from_jd_tt(jd?);
                let seen = obs.as_ref().zip(loc.as_ref()).map(|(o, l)| {
                    let look = o.look(&src, Body::Moon, time.jd_ut);
                    let target =
                        Horizon::STANDARD.target(Body::Moon, look.distance_km, l.elevation_m);
                    (deg(look.alt), look.alt > target)
                });
                Some(LunarContactDto {
                    code: code.into(),
                    label: label.into(),
                    time: ut(time),
                    moon_alt_deg: seen.map(|s| s.0),
                    above_horizon: seen.map(|s| s.1),
                })
            })
            .collect();
            let visible = loc.map(|_| contacts.iter().any(|c| c.above_horizon == Some(true)));
            let kind = lunar_kind(d.kind);
            EclipseDto {
                body: "lunar".into(),
                kind: kind.into(),
                label: format!("Lunar {kind}"),
                lunation: m.lunation,
                greatest: ut_tt(d.jd_tt),
                gamma: d.gamma,
                magnitude: d.umbral_magnitude,
                solar: None,
                lunar: Some(LunarEclipseDto {
                    penumbral_magnitude: d.penumbral_magnitude,
                    umbral_magnitude: d.umbral_magnitude,
                    penumbral_duration_min: d.penumbral_duration_min(),
                    partial_duration_min: d.partial_duration_min(),
                    total_duration_min: d.total_duration_min(),
                    zenith_lat_deg: d.zenith_lat_deg,
                    zenith_lon_deg: d.zenith_lon_deg,
                    contacts,
                    visible,
                }),
            }
        }
    }
}

/// Every eclipse of the years with details, in one call.
pub fn eclipses(eph: &dyn Ephemeris, p: &EclipsesParams) -> Result<EclipseList, ApiError> {
    let list = eclipse_candidates(eph, p)?;
    Ok(EclipseList {
        first_year: p.first_year,
        last_year: p.last_year,
        location: p.location,
        eclipses: list
            .iter()
            .map(|e| eclipse_details(eph, e, p.location))
            .collect(),
        ephemeris: eph.name().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eph() -> std::sync::Arc<dyn Ephemeris> {
        crate::ephemeris::meeus(false)
    }

    fn t(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> UtDateTime {
        UtDateTime {
            year,
            month,
            day,
            hour,
            minute,
            second: 0.0,
        }
    }

    fn minutes_between(a: &UtDateTime, b: &UtDateTime) -> f64 {
        (a.to_instant().jd_ut - b.to_instant().jd_ut) * 1440.0
    }

    const DALLAS: Location = Location {
        lat_deg: 32.78,
        lon_deg: -96.80,
        elevation_m: 140.0,
    };

    #[test]
    fn table_rows_match_meeus_example_47a() {
        // Meeus example 47.a: 1992 April 12, 0h TD: Moon λ = 133.162655°,
        // β = −3.229126°, Δ = 368 409.7 km. At 0h UT the Moon is ΔT ≈ 59 s
        // further on (≈ 0.009°).
        let e = eph();
        let p = EphemerisTableParams {
            start: t(1992, 4, 12, 0, 0),
            step_minutes: 60.0,
            count: 25,
            location: Some(DALLAS),
            ephemeris_id: None,
        };
        let table = ephemeris_table(&*e, &p).unwrap();
        assert_eq!(table.rows.len(), 25);
        let r = &table.rows[0];
        assert!((r.moon.lon_deg - 133.16).abs() < 0.03, "{}", r.moon.lon_deg);
        assert!((r.moon.lat_deg + 3.229).abs() < 0.01);
        assert!((r.moon.distance_km - 368_410.0).abs() < 50.0);
        assert!(r.sun.topocentric.is_some());
        assert_eq!(table.rows[24].time.day, 13);
        // Chunks reproduce the whole table.
        let chunks = table_row_chunks(25, 10);
        assert_eq!(chunks, vec![0..10, 10..20, 20..25]);
        let joined: Vec<EphemerisRowDto> = chunks
            .into_iter()
            .flat_map(|c| table_rows(&*e, &p, c))
            .collect();
        assert_eq!(joined.len(), table.rows.len());
        for (a, b) in joined.iter().zip(&table.rows) {
            // Equal up to the rounding of the chunk's start instant.
            assert!((a.jd_ut - b.jd_ut).abs() < 1e-9);
            assert!((a.moon.lon_deg - b.moon.lon_deg).abs() < 1e-6);
            assert!((a.moon_age_days - b.moon_age_days).abs() < 1e-6);
        }
        // Limits.
        let too_many = EphemerisTableParams {
            count: MAX_TABLE_ROWS + 1,
            ..p.clone()
        };
        assert!(check_table(&*e, &too_many).is_err());
        let no_step = EphemerisTableParams {
            step_minutes: 0.0,
            ..p
        };
        assert!(check_table(&*e, &no_step).is_err());
    }

    #[test]
    fn rise_set_mecca_march_2024() {
        // USNO for Makkah (21.42° N, 39.83° E, UT+3), 2024 March 11:
        // sunrise 06:31, sunset 18:28 local.
        let e = eph();
        let m = rise_set_month(
            &*e,
            &RiseSetParams {
                year: 2024,
                month: 3,
                tz_offset_hours: 3.0,
                location: Location {
                    lat_deg: 21.4225,
                    lon_deg: 39.8262,
                    elevation_m: 0.0,
                },
                ephemeris_id: None,
            },
        )
        .unwrap();
        assert_eq!(m.days.len(), 31);
        let d = &m.days[10];
        assert_eq!(d.date.day, 11);
        let rise = d.sun.rise.unwrap();
        let set = d.sun.set.unwrap();
        assert!(minutes_between(&rise.time, &t(2024, 3, 11, 3, 31)).abs() < 2.0);
        assert!(minutes_between(&set.time, &t(2024, 3, 11, 15, 28)).abs() < 2.0);
        // Near the equinox the Sun rises a little south of east.
        assert!((rise.angle_deg - 93.0).abs() < 3.0, "{}", rise.angle_deg);
        let transit = d.sun.transit.unwrap();
        // Meridian altitude ≈ 90° − 21.4° − 3.9° ≈ 64.7°.
        assert!((transit.angle_deg - 64.7).abs() < 1.0);
        assert!(d.civil.dawn.is_some() && d.astronomical.dusk.is_some());
        assert!(d.moon.circumpolar.is_none() || d.moon.rise.is_none());
    }

    #[test]
    fn phases_of_2024() {
        // New Moons of 2024 (NASA): Jan 11 11:57, ..., Dec 30 22:27 UT; 13
        // new Moons and 12 full Moons.
        let y = lunar_phases(
            &*eph(),
            &YearParams {
                year: 2024,
                tz_offset_hours: 0.0,
                ephemeris_id: None,
            },
        )
        .unwrap();
        let new: Vec<&UtDateTime> = y
            .lunations
            .iter()
            .filter_map(|l| l.phases[0].as_ref())
            .collect();
        assert_eq!(new.len(), 13);
        assert!(minutes_between(new[0], &t(2024, 1, 11, 11, 57)).abs() < 2.0);
        assert!(minutes_between(new[12], &t(2024, 12, 30, 22, 27)).abs() < 2.0);
        let full = y.lunations.iter().filter(|l| l.phases[2].is_some()).count();
        assert_eq!(full, 12);
        for l in &y.lunations {
            assert!((29.2..29.9).contains(&l.length_days));
            if let Some(a) = l.ages_days[2] {
                assert!((13.5..16.0).contains(&a));
            }
        }
    }

    #[test]
    fn apsides_and_nodes_of_2024() {
        let y = apsides_nodes(
            &*eph(),
            &YearParams {
                year: 2024,
                tz_offset_hours: 0.0,
                ephemeris_id: None,
            },
        )
        .unwrap();
        // About 13 perigees, 13 apogees, 27 node passages, 27 extremes.
        assert!((25..=28).contains(&y.apsides.len()));
        assert!((26..=28).contains(&y.nodes.len()));
        assert!((26..=28).contains(&y.declinations.len()));
        // Closest perigee of 2024: March 10, 356 895 km; the October 17
        // perigee is 357 175 km (Espenak, Lunar Perigee and Apogee
        // Calculator, astropixels.com).
        let perigees: Vec<&ApsisDto> = y.apsides.iter().filter(|a| a.kind == "perigee").collect();
        let closest = perigees
            .iter()
            .min_by(|a, b| a.distance_km.total_cmp(&b.distance_km))
            .unwrap();
        assert_eq!((closest.time.month, closest.time.day), (3, 10));
        assert!((closest.distance_km - 356_895.0).abs() < 30.0);
        let october = perigees.iter().find(|a| a.time.month == 10).unwrap();
        assert!((october.distance_km - 357_175.0).abs() < 30.0);
        assert!(closest.diameter_arcmin > 33.0);
        // 2024–2025 is a major lunar standstill: |dec| reaches 28°.
        assert!(y.declinations.iter().any(|d| d.dec_deg.abs() > 28.0));
        for w in y.declinations.windows(2) {
            assert!(w[0].time.to_instant().jd_ut <= w[1].time.to_instant().jd_ut);
        }
    }

    #[test]
    fn seasons_of_2024() {
        // March equinox 2024 Mar 20 03:06 UT; December solstice Dec 21
        // 09:20 UT (USNO).
        let s = seasons(
            &*eph(),
            &YearParams {
                year: 2024,
                tz_offset_hours: 0.0,
                ephemeris_id: None,
            },
        )
        .unwrap();
        assert_eq!(s.seasons.len(), 4);
        assert!(minutes_between(&s.seasons[0].time, &t(2024, 3, 20, 3, 6)).abs() < 2.0);
        assert!(minutes_between(&s.seasons[3].time, &t(2024, 12, 21, 9, 20)).abs() < 2.0);
        assert_eq!(s.equation_of_time.len(), 366);
        // The equation of time peaks at ≈ +16.4 min in early November and
        // dips to ≈ −14.2 min in mid-February.
        let max = s
            .equation_of_time
            .iter()
            .max_by(|a, b| a.minutes.total_cmp(&b.minutes))
            .unwrap();
        let min = s
            .equation_of_time
            .iter()
            .min_by(|a, b| a.minutes.total_cmp(&b.minutes))
            .unwrap();
        assert_eq!(max.date.month, 11);
        assert!((max.minutes - 16.4).abs() < 0.2, "{}", max.minutes);
        assert_eq!(min.date.month, 2);
        assert!((min.minutes + 14.2).abs() < 0.2, "{}", min.minutes);
    }

    #[test]
    fn eclipses_of_2024_at_dallas() {
        // 2024: penumbral lunar (Mar 25), total solar (Apr 8), partial
        // lunar (Sep 18), annular solar (Oct 2).
        let list = eclipses(
            &*eph(),
            &EclipsesParams {
                first_year: 2024,
                last_year: 2024,
                filter: EclipseKindFilter::Any,
                location: Some(DALLAS),
                ephemeris_id: None,
            },
        )
        .unwrap();
        let labels: Vec<&str> = list.eclipses.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Lunar penumbral",
                "Solar total",
                "Lunar partial",
                "Solar annular"
            ]
        );
        // Dallas is in the path of totality on 2024 April 8: maximum at
        // about 18:42:40 UT (NASA: totality 18:40:44–18:44:39).
        let total = &list.eclipses[1];
        let local = total.solar.as_ref().unwrap().local.as_ref().unwrap();
        assert_eq!(local.kind, "total");
        assert!(local.visible);
        assert!(local.obscuration > 0.999);
        let nasa = UtDateTime {
            second: 40.0,
            ..t(2024, 4, 8, 18, 42)
        };
        assert!(minutes_between(&local.max.time, &nasa).abs() < 1.0);
        let totality = local.central_duration_min.unwrap();
        assert!((totality - 3.92).abs() < 0.2, "{totality}");
        assert!((63.5..64.7).contains(&local.max.sun_alt_deg));
        // The October annular eclipse is not seen from Dallas.
        assert!(list.eclipses[3].solar.as_ref().unwrap().local.is_none());
        // The penumbral lunar eclipse of March 25 is visible from Dallas.
        let lunar = list.eclipses[0].lunar.as_ref().unwrap();
        assert_eq!(lunar.visible, Some(true));
        assert_eq!(lunar.contacts.len(), 3);
        let max = lunar.contacts.iter().find(|c| c.code == "max").unwrap();
        assert!(max.moon_alt_deg.unwrap() > 30.0);
        // Without a location there are no local circumstances.
        let bare = eclipses(
            &*eph(),
            &EclipsesParams {
                first_year: 2024,
                last_year: 2024,
                filter: EclipseKindFilter::Solar,
                location: None,
                ephemeris_id: None,
            },
        )
        .unwrap();
        assert_eq!(bare.eclipses.len(), 2);
        assert!(bare.eclipses[0].solar.as_ref().unwrap().local.is_none());
        // Limits.
        assert!(
            eclipse_candidates(
                &*eph(),
                &EclipsesParams {
                    first_year: 2024,
                    last_year: 2024 + MAX_ECLIPSE_YEARS,
                    filter: EclipseKindFilter::Any,
                    location: None,
                    ephemeris_id: None,
                }
            )
            .is_err()
        );
    }

    #[test]
    fn wire_shapes() {
        let p: EclipsesParams = serde_json::from_value(serde_json::json!({
            "firstYear": 2024, "lastYear": 2025, "filter": "solar"
        }))
        .unwrap();
        assert_eq!(p.filter, EclipseKindFilter::Solar);
        assert_eq!(p.location, None);
        let y: YearParams = serde_json::from_value(serde_json::json!({ "year": 2024 })).unwrap();
        assert_eq!(y.tz_offset_hours, 0.0);
    }
}
