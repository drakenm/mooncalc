//! Validation of the visibility- and rule-based Hijri calendars
//! (`mooncalc_visibility::calendar`) against the official Umm al-Qura table
//! and the dates announced in Saudi Arabia (`tests/fixtures/hijri/`,
//! described in `docs/validation.md`). Run with `--nocapture` to see the
//! comparison reports.

use std::sync::Arc;
use std::time::Instant as Clock;

use mooncalc_core::{Ephemeris, MeeusEphemeris};
use mooncalc_hijri::civil::{CivilCalendar, CivilDate};
use mooncalc_hijri::{HijriCalendar, MonthStartProvider, ObservedCalendar, UMM_AL_QURA};
use mooncalc_visibility::calendar::{
    MECCA, Observers, RegionPreset, RuleCalendar, VisibilityProvider,
};
use mooncalc_visibility::criteria::{Odeh, Yallop};

const ANNOUNCED: &str =
    include_str!("../../../tests/fixtures/hijri/saudi_announced_vs_ummalqura.csv");

fn eph() -> Arc<dyn Ephemeris> {
    Arc::new(MeeusEphemeris::new())
}

fn iso_to_jdn(s: &str) -> i64 {
    let mut it = s.split('-').map(|p| p.parse::<i64>().unwrap());
    let (y, m, d) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
    CivilDate::new(y as i32, m as u8, d as u8)
        .to_jdn(CivilCalendar::Gregorian)
        .unwrap()
}

fn format_jdn(jdn: i64) -> String {
    let c = CivilDate::from_jdn(jdn, CivilCalendar::Gregorian);
    format!("{:04}-{:02}-{:02}", c.year, c.month, c.day)
}

/// Announced month starts: (year, month, announced JDN of day 1). Rows for
/// day 10 of Dhu ʾl-Ḥijja are converted to day 1.
fn announced() -> Vec<(i32, u8, i64)> {
    ANNOUNCED
        .lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            let announced = f.get(4).copied().unwrap_or("").trim();
            if announced.is_empty() {
                return None;
            }
            let (y, m, d): (i32, u8, i64) = (
                f[0].parse().unwrap(),
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
            );
            Some((y, m, iso_to_jdn(announced) - (d - 1)))
        })
        .collect()
}

/// The Umm al-Qura rule computed with the Meeus ephemeris against the
/// official table, every month of 1420–1447 AH.
#[test]
fn umm_al_qura_rule_matches_the_official_table() {
    let clock = Clock::now();
    let cal = ObservedCalendar::new(
        RuleCalendar::UmmAlQuraRule.provider(eph()),
        "rule",
        "rule",
        1420,
        1447,
    );
    let mut mismatches = Vec::new();
    let mut total = 0;
    for y in 1420..=1447 {
        assert!(cal.year_length(y).is_some(), "{y}: invalid month length");
        for m in 1..=12 {
            let ours = cal.month_start(y, m).unwrap();
            let table = UMM_AL_QURA.month_start(y, m).unwrap();
            total += 1;
            if ours != table {
                mismatches.push((y, m, ours - table));
            }
        }
    }
    println!(
        "Umm al-Qura rule vs official table, 1420–1447 AH: {}/{total} months agree ({:.1} s)",
        total - mismatches.len(),
        clock.elapsed().as_secs_f64()
    );
    for (y, m, d) in &mismatches {
        println!(
            "  {y}/{m:02}: rule {} vs table {} ({d:+} d)",
            format_jdn(cal.month_start(*y, *m).unwrap()),
            format_jdn(UMM_AL_QURA.month_start(*y, *m).unwrap())
        );
    }
    assert!(mismatches.len() <= 6, "{mismatches:?}");

    // The simpler conjunction rules, for comparison.
    for rule in [
        RuleCalendar::ConjunctionBeforeSunsetMecca,
        RuleCalendar::ConjunctionBeforeNoonUt,
    ] {
        let c = ObservedCalendar::new(rule.provider(eph()), "r", "r", 1420, 1447);
        let agree = (1420..=1447)
            .flat_map(|y| (1..=12).map(move |m| (y, m)))
            .filter(|&(y, m)| c.month_start(y, m) == UMM_AL_QURA.month_start(y, m))
            .count();
        println!(
            "  {}: {agree}/{total} months agree with the table",
            rule.name()
        );
        for y in 1420..=1447 {
            assert!(c.year_length(y).is_some(), "{rule:?} {y}");
        }
    }
    assert!(mismatches.iter().all(|&(_, _, d)| d.abs() == 1));
}

/// Agreement of a calendar with the announced Saudi dates.
fn agreement(cal: &dyn Fn(i32, u8) -> Option<i64>, rows: &[(i32, u8, i64)]) -> (usize, Vec<i64>) {
    let mut agree = 0;
    let mut diffs = Vec::new();
    for &(y, m, jdn) in rows {
        let d = cal(y, m).unwrap() - jdn;
        if d == 0 {
            agree += 1;
        }
        diffs.push(d);
    }
    (agree, diffs)
}

fn histogram(diffs: &[i64]) -> String {
    (-2..=2)
        .map(|d| format!("{d:+}: {}", diffs.iter().filter(|&&x| x == d).count()))
        .collect::<Vec<_>>()
        .join("  ")
}

/// Visibility at Mecca (Yallop, Odeh) against the dates announced by the
/// Saudi authorities, 1422–1448 AH. The announcements follow reported
/// sightings, which often precede what the criteria allow, so agreement is
/// far from perfect; the Umm al-Qura table is shown for reference.
#[test]
fn mecca_visibility_against_saudi_announcements() {
    let rows = announced();
    assert!(rows.len() > 90, "{}", rows.len());
    let clock = Clock::now();
    let (uaq, uaq_d) = agreement(&|y, m| UMM_AL_QURA.month_start(y, m), &rows);
    println!(
        "Saudi announced month starts, {} months 1422–1448 AH (difference = calendar − announced):",
        rows.len()
    );
    println!(
        "  Umm al-Qura table          {uaq:3} agree ({:4.1}%)  {}",
        100.0 * uaq as f64 / rows.len() as f64,
        histogram(&uaq_d)
    );
    let place = Observers::Place(MECCA);
    let mut best = 0.0f64;
    for (name, criterion, zone) in [
        ("Yallop ≤ A", Arc::new(Yallop) as Arc<_>, 0),
        ("Yallop ≤ B (naked eye)", Arc::new(Yallop) as Arc<_>, 1),
        ("Yallop ≤ C", Arc::new(Yallop) as Arc<_>, 2),
        ("Yallop ≤ D (optical aid)", Arc::new(Yallop) as Arc<_>, 3),
        ("Odeh ≤ C (optical aid)", Arc::new(Odeh) as Arc<_>, 2),
    ] {
        let p = VisibilityProvider::new(eph(), criterion, zone, &place);
        let cal = ObservedCalendar::new(p, "v", "v", 1422, 1448);
        let (agree, d) = agreement(&|y, m| cal.month_start(y, m), &rows);
        let rate = agree as f64 / rows.len() as f64;
        best = best.max(rate);
        println!(
            "  Mecca {name:<24} {agree:3} agree ({:4.1}%)  {}",
            100.0 * rate,
            histogram(&d)
        );
        // Every month is 29 or 30 days, and none is far from the announced.
        for y in 1422..=1448 {
            assert!(cal.year_length(y).is_some(), "{name} {y}");
        }
        assert!(d.iter().all(|x| x.abs() <= 2), "{name}: {d:?}");
    }
    println!("  ({:.1} s)", clock.elapsed().as_secs_f64());
    // The announcements are usually a day ahead of the criteria; the best
    // threshold still agrees on a substantial share.
    assert!(best > 0.3, "{best}");
}

/// Regional calendars keep the 29/30 contract, and "anywhere in the world"
/// does not start later than a sub-region (up to grid sampling).
#[test]
fn regional_calendars() {
    let clock = Clock::now();
    let e = eph();
    let make = |preset: RegionPreset| {
        let observers = Observers::Region {
            region: preset.region(),
            step_deg: 10.0,
        };
        ObservedCalendar::new(
            VisibilityProvider::new(e.clone(), Arc::new(Yallop), 1, &observers),
            preset.id(),
            preset.name(),
            1446,
            1446,
        )
    };
    let world = make(RegionPreset::World);
    let regions: Vec<_> = RegionPreset::ALL[1..].iter().map(|&r| make(r)).collect();
    for m in 1..=12 {
        let w = world.month_start(1446, m).unwrap();
        let uaq = UMM_AL_QURA.month_start(1446, m).unwrap();
        let line: Vec<String> = regions
            .iter()
            .map(|c| {
                let s = c.month_start(1446, m).unwrap();
                // The world grid is not a superset of every region's grid
                // points, so allow a day for sampling.
                assert!(s >= w - 1, "month {m}");
                format!("{:+}", s - uaq)
            })
            .collect();
        println!(
            "1446/{m:02}: world {:+}  regions {}  (days from Umm al-Qura)",
            w - uaq,
            line.join(" ")
        );
    }
    assert!(world.year_length(1446).is_some());
    for c in &regions {
        assert!(c.year_length(1446).is_some(), "{}", c.id());
    }
    println!(
        "regional calendars, one year: {:.1} s",
        clock.elapsed().as_secs_f64()
    );
}

/// The provider works month by month in any order, with the same results.
#[test]
fn month_alignment_is_stable() {
    let e = eph();
    let p = VisibilityProvider::new(e.clone(), Arc::new(Yallop), 1, &Observers::Place(MECCA));
    let q = VisibilityProvider::new(e, Arc::new(Yallop), 1, &Observers::Place(MECCA));
    // Far-apart queries first, then the neighbourhood: the same answers.
    let far = q.month_start(1447, 9);
    for y in [1300, 1445, 1600] {
        for m in [1, 9] {
            let s = p.month_start(y, m).unwrap();
            // Each month starts 0–4 days after its own conjunction's UT date,
            // so it cannot be a month off.
            let k = mooncalc_hijri::chained::lunation_of_month(y, m);
            let mean = mooncalc_hijri::newmoon::mean_new_moon(k);
            assert!((-1.0..5.0).contains(&(s as f64 - mean)), "{y}/{m}");
        }
    }
    assert_eq!(p.month_start(1447, 9), far);
}
