//! Rise/set/transit/twilight and lunar phases against USNO
//! (`tests/fixtures/riseset/usno_rstt.csv`, `tests/fixtures/phases/usno_moon_phases.csv`;
//! see `docs/validation.md`).

use std::collections::BTreeMap;

use mooncalc_core::GeoLocation;
use mooncalc_core::MeeusEphemeris;
use mooncalc_core::events::{
    Body, CalendarDate, Circumpolar, DirectSource, EventKind, Horizon, Phase, Searcher, Twilight,
    day_events, phase_near,
};
use mooncalc_core::time::Instant;

const RSTT: &str = include_str!("../../../tests/fixtures/riseset/usno_rstt.csv");
const PHASES: &str = include_str!("../../../tests/fixtures/phases/usno_moon_phases.csv");

fn parse_date(s: &str) -> CalendarDate {
    let p: Vec<u32> = s.split('-').map(|x| x.parse().unwrap()).collect();
    CalendarDate::new(p[0] as i32, p[1], p[2])
}

fn minutes(s: &str) -> f64 {
    let (h, m) = s.split_once(':').unwrap();
    h.parse::<f64>().unwrap() * 60.0 + m.parse::<f64>().unwrap()
}

/// One site-date-body group of USNO rows.
#[derive(Default)]
struct Group {
    loc: Option<GeoLocation>,
    events: Vec<(String, Option<f64>)>,
}

#[test]
fn usno_rise_set_transit_twilight() {
    let eph = MeeusEphemeris::new();
    let src = DirectSource::new(&eph);
    let mut groups: BTreeMap<(String, String, String), Group> = BTreeMap::new();
    for line in RSTT.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f[5] == "fracillum" {
            continue;
        }
        let g = groups
            .entry((f[0].to_string(), f[3].to_string(), f[4].to_string()))
            .or_default();
        g.loc = Some(GeoLocation::new(
            f[2].parse().unwrap(),
            f[1].parse().unwrap(),
            0.0,
        ));
        g.events
            .push((f[5].to_string(), (!f[6].is_empty()).then(|| minutes(f[6]))));
    }
    let mut worst: BTreeMap<&str, f64> = BTreeMap::new();
    let mut compared = 0;
    for ((site, date, body), g) in &groups {
        let loc = g.loc.unwrap();
        let body_e = if body == "sun" { Body::Sun } else { Body::Moon };
        let d = parse_date(date);
        let (t0, t1) = d.local_day(0.0);
        let s = Searcher::new(&src, body_e, &loc, Horizon::STANDARD);
        let ours = s.events(t0.jd_ut, t1.jd_ut);
        let tw = Searcher::new(&src, Body::Sun, &loc, Horizon::twilight(Twilight::Civil));
        let tw_ev = tw.crossings(t0.jd_ut, t1.jd_ut);
        let polar_tol: f64 = if site == "tromso" { 5.0 } else { 1.0 };
        let tol = if body_e == Body::Moon { 2.0 } else { polar_tol };
        let mins = |t: Instant| (t.jd_ut - t0.jd_ut) * 1440.0;
        for (ev, time) in &g.events {
            let (kind, list, label) = match ev.as_str() {
                "rise" => (EventKind::Rise, &ours, "rise/set"),
                "set" => (EventKind::Set, &ours, "rise/set"),
                "upper_transit" => (EventKind::UpperTransit, &ours, "transit"),
                "lower_transit" => (EventKind::LowerTransit, &ours, "transit"),
                "civil_twilight_begin" => (EventKind::Rise, &tw_ev, "twilight"),
                "civil_twilight_end" => (EventKind::Set, &tw_ev, "twilight"),
                "continuously_above_horizon" | "continuously_below_horizon" => {
                    let de = day_events(&src, body_e, &loc, Horizon::STANDARD, t0, t1);
                    let expect = if ev == "continuously_above_horizon" {
                        Circumpolar::AlwaysUp
                    } else {
                        Circumpolar::AlwaysDown
                    };
                    assert_eq!(de.circumpolar, Some(expect), "{site} {date} {body} {ev}");
                    continue;
                }
                "continuously_above_twilight_limit" => {
                    assert!(
                        tw_ev.is_empty(),
                        "{site} {date}: twilight crossings {tw_ev:?}"
                    );
                    assert!(tw.margin(t0.jd_ut).0 > 0.0);
                    continue;
                }
                other => panic!("unknown event {other}"),
            };
            let want = time.unwrap();
            let best = list
                .iter()
                .filter(|e| e.kind == kind)
                .map(|e| mins(e.time) - want)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
                .unwrap_or_else(|| panic!("{site} {date} {body}: no {ev} found"));
            let t = if label == "rise/set" {
                tol
            } else {
                polar_tol.max(tol)
            };
            assert!(best.abs() <= t, "{site} {date} {body} {ev}: {best:+.2} min");
            let key = match (label, site == "tromso", body_e) {
                ("rise/set", true, _) => "rise/set Tromsø",
                ("rise/set", false, Body::Sun) => "rise/set Sun",
                ("rise/set", false, Body::Moon) => "rise/set Moon",
                _ => label,
            };
            let w = worst.entry(key).or_default();
            *w = w.max(best.abs());
            compared += 1;
        }
        // Every rise/set we find must be in USNO's list, unless it falls
        // within a minute of the day boundary (USNO rounds to the minute).
        for e in ours
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Rise | EventKind::Set))
        {
            let m = mins(e.time);
            let name = if e.kind == EventKind::Rise {
                "rise"
            } else {
                "set"
            };
            let listed = g.events.iter().any(|(n, _)| n == name);
            assert!(
                listed || !(1.0..1439.0).contains(&m),
                "{site} {date} {body}: extra {name} at {m:.1}"
            );
        }
    }
    assert!(compared > 450, "{compared}");
    println!("USNO rise/set/transit: {compared} events; worst |Δ| (min): {worst:?}");
}

#[test]
fn usno_moon_phases_2000_2030() {
    let eph = MeeusEphemeris::new();
    let mut n = 0;
    let mut worst = 0.0_f64;
    let mut sum = 0.0;
    for line in PHASES.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let d = parse_date(f[0]);
        if d.year < 2000 {
            continue;
        }
        let phase = match f[2] {
            "New Moon" => Phase::New,
            "First Quarter" => Phase::FirstQuarter,
            "Full Moon" => Phase::Full,
            "Last Quarter" => Phase::LastQuarter,
            other => panic!("{other}"),
        };
        let usno = Instant::from_jd_ut(d.jd0() + minutes(f[1]) / 1440.0);
        let ours = phase_near(&eph, phase, usno);
        let diff = (ours.jd_ut - usno.jd_ut) * 1440.0;
        worst = worst.max(diff.abs());
        sum += diff.abs();
        n += 1;
        assert!(diff.abs() <= 1.0, "{line}: {diff:+.2} min");
    }
    assert!(n > 1500, "{n}");
    println!(
        "USNO phases 2000-2030: {n}, worst {worst:.2} min, mean {:.2} min",
        sum / n as f64
    );
}
