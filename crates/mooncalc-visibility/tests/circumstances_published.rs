//! End-to-end checks of `circumstances` against published tables: Yallop
//! (1997) Table 4, Odeh (2004) Table VI and HMNAO's city predictions
//! (`tests/fixtures/`, described in `docs/validation.md`). Run with
//! `--nocapture` to see the residual statistics.

use mooncalc_core::events::{ApparentTable, CalendarDate, Horizon, Observer};
use mooncalc_core::time::Instant;
use mooncalc_core::{GeoLocation, MeeusEphemeris};
use mooncalc_visibility::circumstances::{
    Outcome, Request, Settings, Sighting, compute, compute_with, geometry_at, new_moons_around,
};
use mooncalc_visibility::criteria::{EvaluationTime, Yallop, odeh, yallop};

const YALLOP: &str = include_str!("../../../tests/fixtures/crescent/yallop1997_table4.csv");
const ODEH: &str = include_str!("../../../tests/fixtures/crescent/odeh2004_table6.csv");
const HMNAO: &str = include_str!("../../../tests/fixtures/predictions/hmnao_city_visibility.csv");

/// Summary statistics of absolute residuals.
#[derive(Default)]
struct Stats {
    v: Vec<f64>,
}

impl Stats {
    fn push(&mut self, x: f64) {
        self.v.push(x.abs());
    }
    fn sorted(&self) -> Vec<f64> {
        let mut s = self.v.clone();
        s.sort_by(f64::total_cmp);
        s
    }
    fn median(&self) -> f64 {
        let s = self.sorted();
        s[s.len() / 2]
    }
    fn p90(&self) -> f64 {
        let s = self.sorted();
        s[(s.len() * 9) / 10]
    }
    fn max(&self) -> f64 {
        self.sorted().last().copied().unwrap_or(0.0)
    }
    fn fraction_within(&self, tol: f64) -> f64 {
        self.v.iter().filter(|&&x| x <= tol).count() as f64 / self.v.len() as f64
    }
    fn line(&self, name: &str) -> String {
        format!(
            "{name:>6}: median {:.3}  p90 {:.3}  max {:.3}",
            self.median(),
            self.p90(),
            self.max()
        )
    }
}

fn evening_or_morning(me: &str) -> Sighting {
    if me == "M" {
        Sighting::Morning
    } else {
        Sighting::Evening
    }
}

#[test]
fn yallop_table4_end_to_end() {
    let eph = MeeusEphemeris::new();
    let settings = Settings::default();
    let [mut arcl, mut arcv, mut daz, mut w, mut q, mut lag, mut age] =
        std::array::from_fn(|_| Stats::default());
    let mut zone_match = 0;
    let mut n = 0;
    let mut unmatched = Vec::new();
    let mut shifted = Vec::new();
    for line in YALLOP.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let no: u32 = f[0].parse().unwrap();
        let date = CalendarDate::new(
            f[1].parse().unwrap(),
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
        );
        let p = |i: usize| f[i].parse::<f64>().unwrap();
        let loc = GeoLocation::new(p(6), p(7), 0.0);
        let at = |d: CalendarDate| {
            let req = Request::local_date(
                loc,
                d,
                evening_or_morning(f[4]),
                EvaluationTime::YallopBestTime,
            );
            compute(&eph, &req, &settings)
        };
        let age_off = |o: &Outcome| {
            o.geometry()
                .is_none_or(|g| (g.age_hours - p(11).abs()).abs() > 6.0)
        };
        let o = at(date);
        if age_off(&o) {
            // A few records in the source list carry the UT date rather than
            // the local one (TN 69 §5 says so of No. 285). Identify them by
            // trying the neighbouring dates, and keep them out of the stats.
            match [date.add_days(-1), date.add_days(1)]
                .into_iter()
                .find(|&d| !age_off(&at(d)))
            {
                Some(d) => shifted.push(format!(
                    "No. {no} {} → {}-{}-{}",
                    f[1..4].join("-"),
                    d.year,
                    d.month,
                    d.day
                )),
                None => unmatched.push(format!("No. {no}: {:?}", o.status())),
            }
            continue;
        }
        let Outcome::Crescent(c) = o else {
            unmatched.push(format!("No. {no}: {:?}", o.status()));
            continue;
        };
        let g = &c.geometry;
        n += 1;
        let qv = yallop::q(g.arcv_deg, g.width_topo_arcmin);
        arcl.push(g.arcl_geo_deg - p(8));
        arcv.push(g.arcv_deg - p(9));
        daz.push(g.daz_deg - p(10));
        age.push(g.age_hours - p(11).abs());
        lag.push(g.lag_minutes - p(12).abs());
        w.push(g.width_topo_arcmin - p(14));
        q.push(qv - p(15));
        if ["A", "B", "C", "D", "E", "F"][yallop::zone_for_q(qv)] == f[18] {
            zone_match += 1;
        }
    }
    println!("Yallop Table 4: {n} records on their printed date");
    for s in [
        arcl.line("ARCL°"),
        arcv.line("ARCV°"),
        daz.line("DAZ°"),
        w.line("W′ ′"),
        q.line("q"),
        lag.line("lag m"),
        age.line("age h"),
    ] {
        println!("  {s}");
    }
    println!(
        "  within doc tolerances: ARCL≤0.2 {:.1}%  ARCV≤0.2 {:.1}%  DAZ≤0.2 {:.1}%  q≤0.02 {:.1}%  lag≤2 {:.1}%  age≤0.2 {:.1}%  zone {zone_match}/{n}",
        100.0 * arcl.fraction_within(0.2),
        100.0 * arcv.fraction_within(0.2),
        100.0 * daz.fraction_within(0.2),
        100.0 * q.fraction_within(0.02),
        100.0 * lag.fraction_within(2.0),
        100.0 * age.fraction_within(0.2),
    );
    println!("  date-shifted records: {shifted:?}");
    println!("  unmatched: {unmatched:?}");
    assert!(n >= 285, "{n}");
    assert!(
        unmatched.is_empty() && shifted.len() <= 6,
        "{unmatched:?} {shifted:?}"
    );
    assert!(arcv.median() < 0.05 && arcv.max() < 0.2);
    assert!(arcl.median() < 0.05 && arcl.max() < 0.2);
    assert!(daz.median() < 0.05 && daz.max() < 0.2);
    assert!(q.median() < 0.005 && q.fraction_within(0.02) > 0.98);
    assert!(w.median() < 0.01);
    assert!(lag.median() < 0.5 && lag.max() < 2.0);
    assert!(age.median() < 0.1 && age.max() < 0.2);
    assert!(zone_match + 2 >= n);
}

#[test]
fn odeh_table6_geometry_and_best_time() {
    let eph = MeeusEphemeris::new();
    // Odeh's sunset and moonset include the dip of the horizon for the
    // site's elevation (his best times match ours to a median 0.6 min with
    // the dip, 3 min without).
    let settings = Settings {
        horizon: Horizon::UpperLimb {
            refraction_arcmin: 34.0,
            dip: true,
        },
        ..Settings::default()
    };
    let [
        mut arcv,
        mut daz,
        mut arcl,
        mut w,
        mut v,
        mut v_yw,
        mut tb,
        mut lag,
    ] = std::array::from_fn(|_| Stats::default());
    let (mut n, mut zone_match, mut timed) = (0, 0, 0);
    for line in ODEH.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let no: u32 = f[0].parse().unwrap();
        if [737, 126, 138, 142, 212].contains(&no) {
            continue; // printed inconsistencies, docs/validation.md
        }
        let p = |i: usize| f[i].parse::<f64>().unwrap();
        let loc = GeoLocation::new(p(6), p(5), p(7));
        // (1) Geometry at Odeh's own best time.
        let jd = p(11);
        let t = Instant::from_jd_ut(jd);
        let table = ApparentTable::with_default_step(&eph, t.add_days(-0.1), t.add_days(0.1));
        let g = geometry_at(
            &table,
            &Observer::new(&loc),
            jd,
            1.0,
            1.0,
            &settings.atmosphere,
        );
        n += 1;
        arcv.push(g.arcv_topo_deg - p(14));
        daz.push(g.daz_deg.abs() - p(15).abs());
        arcl.push(g.arcl_deg - p(16));
        // Odeh's W is topocentric with the topocentric elongation,
        // W = SD′ (1 − cos ARCL′); Yallop's W′ uses the geocentric ARCL.
        let w_odeh = odeh::width_arcmin(&g);
        let vv = odeh::v(g.arcv_topo_deg, w_odeh);
        w.push(w_odeh * 60.0 - p(17));
        v.push(vv - p(18));
        v_yw.push(odeh::v(g.arcv_topo_deg, g.width_topo_arcmin) - p(18));
        if ["A", "B", "C", "D"][odeh::zone_for_v(vv)] == f[19] {
            zone_match += 1;
        }
        // (2) Best time and lag from the printed local date.
        let d: Vec<u32> = f[3].split('-').map(|x| x.parse().unwrap()).collect();
        let date = CalendarDate::new(d[2] as i32, d[1], d[0]);
        let req = Request::local_date(
            loc,
            date,
            evening_or_morning(f[2]),
            EvaluationTime::YallopBestTime,
        );
        if let Some(c) = compute(&eph, &req, &settings).circumstances()
            && c.moon_event.is_some()
            && c.geometry.lag_minutes > 0.0
        {
            timed += 1;
            tb.push((c.evaluation.jd_ut - jd) * 1440.0);
            lag.push(c.geometry.lag_minutes - p(13).abs());
        }
    }
    println!("Odeh Table VI: {n} records at Odeh's best time, {timed} timed from the date");
    for s in [
        arcv.line("ARCV°"),
        daz.line("DAZ°"),
        arcl.line("ARCL°"),
        w.line("W″"),
        v.line("V"),
        v_yw.line("V(W′)"),
        tb.line("Tb m"),
        lag.line("lag m"),
    ] {
        println!("  {s}");
    }
    println!(
        "  within doc tolerances: ARCV≤0.15 {:.1}%  DAZ≤0.15 {:.1}%  ARCL≤0.15 {:.1}%  W≤2″ {:.1}%  V≤0.12 {:.1}%  lag≤2 {:.1}%  zone {zone_match}/{n}",
        100.0 * arcv.fraction_within(0.15),
        100.0 * daz.fraction_within(0.15),
        100.0 * arcl.fraction_within(0.15),
        100.0 * w.fraction_within(2.0),
        100.0 * v.fraction_within(0.12),
        100.0 * lag.fraction_within(2.0),
    );
    assert!(n >= 570 && timed >= 560, "{n} {timed}");
    assert!(arcv.median() < 0.05 && arcv.fraction_within(0.15) > 0.97);
    assert!(daz.median() < 0.05 && daz.fraction_within(0.15) > 0.97);
    assert!(arcl.median() < 0.05 && arcl.fraction_within(0.15) > 0.99);
    assert!(w.median() < 0.6 && w.fraction_within(2.0) > 0.97);
    assert!(v.median() < 0.05 && v.fraction_within(0.12) > 0.9);
    assert!(tb.median() < 1.5 && lag.median() < 1.0);
    assert!(zone_match as f64 >= 0.95 * n as f64);
}

/// City coordinates (approximate city centres; HMNAO does not state theirs).
fn city(name: &str) -> GeoLocation {
    let (lat, lon) = match name {
        "Belfast" => (54.597, -5.930),
        "Birmingham" => (52.486, -1.890),
        "Bradford" => (53.796, -1.759),
        "Cairo" => (30.044, 31.236),
        "Cape Town" => (-33.925, 18.424),
        "Cardiff" => (51.481, -3.179),
        "Dakhla" => (23.684, -15.958),
        "Dhaka" => (23.810, 90.413),
        "Glasgow" => (55.864, -4.252),
        "Istanbul" => (41.008, 28.978),
        "Jakarta" => (-6.208, 106.846),
        "Karachi" => (24.861, 67.010),
        "Lagos" => (6.524, 3.379),
        "Leeds" => (53.801, -1.549),
        "Leicester" => (52.637, -1.140),
        "London" => (51.507, -0.128),
        "Los Angeles" => (34.052, -118.244),
        "Manchester" => (53.481, -2.243),
        "Mecca" => (21.423, 39.826),
        "New York" => (40.713, -74.006),
        "Newcastle" => (54.978, -1.618),
        "Rabat" => (34.020, -6.841),
        "Sheffield" => (53.381, -1.470),
        "York" => (53.960, -1.087),
        other => panic!("unknown city {other}"),
    };
    GeoLocation::new(lat, lon, 0.0)
}

fn ut_minutes_of_day(s: &str) -> (CalendarDate, f64) {
    let (d, t) = s.split_once(' ').unwrap();
    let p: Vec<u32> = d.split('-').map(|x| x.parse().unwrap()).collect();
    let (h, m) = t.split_once(':').unwrap();
    (
        CalendarDate::new(p[0] as i32, p[1], p[2]),
        h.parse::<f64>().unwrap() * 60.0 + m.parse::<f64>().unwrap(),
    )
}

#[test]
fn hmnao_city_predictions_feb_mar_2026() {
    let eph = MeeusEphemeris::new();
    let settings = Settings::default();
    let start = Instant::from_calendar(2026, 2, 14, 0, 0, 0.0);
    let table = ApparentTable::with_default_step(&eph, start, start.add_days(40.0));
    let new_moons = new_moons_around(&eph, start.add_days(20.0));
    let (mut sunset, mut moonset) = (Stats::default(), Stats::default());
    let (mut n, mut exact, mut near) = (0, 0, 0);
    let mut misses = Vec::new();
    for line in HMNAO.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let d: Vec<u32> = f[0].split('-').map(|x| x.parse().unwrap()).collect();
        let date = CalendarDate::new(d[0] as i32, d[1], d[2]);
        // Civil noon in the city's zone anchors the local date.
        let offset: f64 = f[3].parse().unwrap();
        let req = Request {
            location: city(f[1]),
            noon: Instant::from_jd_ut(date.jd0() + 0.5 - offset / 24.0),
            sighting: Sighting::Evening,
            evaluation: EvaluationTime::YallopBestTime,
        };
        let o = compute_with(&table, &new_moons, &req, &settings);
        if !f[8].is_empty() {
            let c = o.circumstances().expect("circumstances");
            let (sd, sm) = ut_minutes_of_day(f[8]);
            sunset.push((c.sun_event.jd_ut - sd.jd0()) * 1440.0 - sm);
            if let (Some(me), false) = (c.moon_event, f[9].is_empty()) {
                let (md, mm) = ut_minutes_of_day(f[9]);
                moonset.push((me.jd_ut - md.jd0()) * 1440.0 - mm);
            }
        }
        let (zone, q) = match o.evaluate(&Yallop) {
            Some(r) => (r.zone, r.value.unwrap()),
            None => (5, f64::NEG_INFINITY),
        };
        let want = "ABCDEF".find(f[5]).unwrap();
        n += 1;
        if zone == want {
            exact += 1;
        } else if zone.abs_diff(want) == 1 && yallop::Q_BOUNDS.iter().any(|b| (q - b).abs() < 0.03)
        {
            near += 1;
        } else {
            misses.push(format!(
                "{} {}: ours {} (q {q:.3}), HMNAO {}",
                f[0],
                f[1],
                "ABCDEF".as_bytes()[zone] as char,
                f[5]
            ));
        }
    }
    println!("HMNAO cities: {n} evenings; category exact {exact}, one step at a boundary {near}");
    println!("  {}  (minutes)", sunset.line("sunset"));
    println!("  {}  (minutes)", moonset.line("moonset"));
    assert!(n == 192 && misses.is_empty(), "{misses:?}");
    assert!(
        sunset.max() <= 2.0 && moonset.max() <= 2.0,
        "{} {}",
        sunset.max(),
        moonset.max()
    );
}
