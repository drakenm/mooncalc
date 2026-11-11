//! Parser and evaluator tests on synthetic SPK files written by
//! `common::build_spk`; no external data needed.

mod common;

use std::io::{Cursor, ErrorKind};
use std::sync::atomic::Ordering;

use common::{CountingReader, SegSpec, build_spk};
use mooncalc_spk::{ByteOrder, Spk, SpkError};

/// Deterministic Chebyshev coefficients for one record.
fn record(mid: f64, radius: f64, components: usize, seed: f64) -> Vec<f64> {
    let mut r = vec![mid, radius];
    for c in 0..components {
        for k in 0..4 {
            let x = seed * 1.37 + c as f64 * 0.71 + k as f64 * 0.29;
            r.push((x.sin() * 1000.0) / (1.0 + k as f64 * k as f64));
        }
    }
    r
}

/// Closed-form value and d/ds of a degree-3 Chebyshev series:
/// T2 = 2s²−1, T3 = 4s³−3s.
fn cheb3(c: &[f64], s: f64) -> (f64, f64) {
    let v = c[0] + c[1] * s + c[2] * (2.0 * s * s - 1.0) + c[3] * (4.0 * s.powi(3) - 3.0 * s);
    let d = c[1] + 4.0 * c[2] * s + c[3] * (12.0 * s * s - 3.0);
    (v, d)
}

/// Expected position and velocity from one record, computed independently.
fn expect(rec: &[f64], components: usize, et: f64) -> ([f64; 3], [f64; 3]) {
    let (mid, radius) = (rec[0], rec[1]);
    let s = (et - mid) / radius;
    let ncoef = (rec.len() - 2) / components;
    let comp = |c: usize| &rec[2 + c * ncoef..2 + (c + 1) * ncoef];
    let mut p = [0.0; 3];
    let mut v = [0.0; 3];
    for k in 0..3 {
        if ncoef == 1 {
            p[k] = comp(k)[0];
            continue;
        }
        let (val, der) = cheb3(comp(k), s);
        p[k] = val;
        v[k] = if components == 6 {
            cheb3(comp(k + 3), s).0
        } else {
            der / radius
        };
    }
    (p, v)
}

/// Segment layout used by most tests (indices in file order):
/// 0 EMB(3)/SSB type 2, two records; 1 Earth(399)/EMB type 3;
/// 2 Moon(301)/EMB type 2; 3 Sun(10)/SSB type 2;
/// 4 Moon/EMB type 2 over a later span; 5 Moon/EMB constant, overriding
/// segment 2 on [-200, 200]; 6 body 499 type 13; 7 body 599 in frame 17.
fn segments() -> Vec<SegSpec> {
    let seg = |name, target, center, frame, data_type, start, end, init, intlen, records| SegSpec {
        name,
        target,
        center,
        frame,
        data_type,
        start_et: start,
        end_et: end,
        init,
        intlen,
        records,
    };
    vec![
        seg(
            "EMB",
            3,
            0,
            1,
            2,
            -1000.0,
            1000.0,
            -1000.0,
            1000.0,
            vec![record(-500.0, 500.0, 3, 1.0), record(500.0, 500.0, 3, 2.0)],
        ),
        seg(
            "EARTH",
            399,
            3,
            1,
            3,
            -1000.0,
            1000.0,
            -1000.0,
            2000.0,
            vec![record(0.0, 1000.0, 6, 3.0)],
        ),
        seg(
            "MOON-A",
            301,
            3,
            1,
            2,
            -1000.0,
            1000.0,
            -1000.0,
            2000.0,
            vec![record(0.0, 1000.0, 3, 4.0)],
        ),
        seg(
            "SUN",
            10,
            0,
            1,
            2,
            -1000.0,
            1000.0,
            -1000.0,
            2000.0,
            vec![record(0.0, 1000.0, 3, 5.0)],
        ),
        seg(
            "MOON-B",
            301,
            3,
            1,
            2,
            1000.0,
            2000.0,
            1000.0,
            1000.0,
            vec![record(1500.0, 500.0, 3, 6.0)],
        ),
        seg(
            "MOON-OVERRIDE",
            301,
            3,
            1,
            2,
            -200.0,
            200.0,
            -200.0,
            400.0,
            vec![vec![0.0, 200.0, 11.0, 22.0, 33.0]],
        ),
        seg(
            "TYPE13",
            499,
            0,
            1,
            13,
            -1000.0,
            1000.0,
            0.0,
            0.0,
            vec![vec![1.0; 7]],
        ),
        seg(
            "ECLIPTIC",
            599,
            0,
            17,
            2,
            -1000.0,
            1000.0,
            -1000.0,
            2000.0,
            vec![record(0.0, 1000.0, 3, 7.0)],
        ),
    ]
}

fn assert_close(a: [f64; 3], b: [f64; 3], tol: f64, what: &str) {
    for k in 0..3 {
        assert!(
            (a[k] - b[k]).abs() <= tol * (1.0 + b[k].abs()),
            "{what}: component {k}: {} vs {}",
            a[k],
            b[k]
        );
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

const TOL: f64 = 1e-13;

#[test]
fn lists_segments_in_both_byte_orders() {
    let specs = segments();
    for big in [false, true] {
        let bytes = build_spk(&specs, big, 3);
        let spk = Spk::from_bytes(bytes).unwrap();
        let fr = spk.file_record();
        assert_eq!(fr.id_word, "DAF/SPK");
        assert_eq!(fr.internal_name, "SYNTHETIC-1");
        assert_eq!((fr.nd, fr.ni), (2, 6));
        assert_eq!(
            spk.byte_order(),
            if big {
                ByteOrder::Big
            } else {
                ByteOrder::Little
            }
        );
        // 8 segments in summary records of 3: three linked records.
        assert_eq!(fr.forward, 2);
        assert_eq!(fr.backward, 6);
        let segs = spk.segments();
        assert_eq!(segs.len(), specs.len());
        for (s, spec) in segs.iter().zip(&specs) {
            assert_eq!(s.name, spec.name);
            assert_eq!(
                (s.target, s.center, s.frame, s.data_type),
                (spec.target, spec.center, spec.frame, spec.data_type)
            );
            assert_eq!((s.start_et, s.end_et), (spec.start_et, spec.end_et));
        }
        assert_eq!(spk.coverage(301), Some((-1000.0, 2000.0)));
        assert_eq!(spk.coverage(42), None);
        assert_eq!(spk.common_coverage(&[0, 301, 399]), Some((-1000.0, 1000.0)));
        assert_eq!(spk.common_coverage(&[301, 42]), None);
    }
}

#[test]
fn evaluates_type2_and_type3_segments() {
    let specs = segments();
    let spk = Spk::from_bytes(build_spk(&specs, false, 25)).unwrap();
    for et in [
        -1000.0, -750.0, -500.0, -0.25, 0.0, 1e-9, 321.5, 999.0, 1000.0,
    ] {
        // Segment 0 has two records split at et = 0 (INIT -1000, INTLEN 1000).
        let rec = if et < 0.0 { 0 } else { 1 };
        let got = spk.segment_state(0, et).unwrap();
        let want = expect(&specs[0].records[rec], 3, et);
        assert_close(got.0, want.0, TOL, "type 2 position");
        assert_close(got.1, want.1, TOL, "type 2 velocity");

        let got = spk.segment_state(1, et).unwrap();
        let want = expect(&specs[1].records[0], 6, et);
        assert_close(got.0, want.0, TOL, "type 3 position");
        assert_close(got.1, want.1, TOL, "type 3 velocity");
    }
}

#[test]
fn chains_through_the_tree() {
    let specs = segments();
    let spk = Spk::from_bytes(build_spk(&specs, false, 25)).unwrap();
    let st = |i: usize, et: f64| {
        let rec = if i == 0 && et >= 0.0 { 1 } else { 0 };
        expect(&specs[i].records[rec], if i == 1 { 6 } else { 3 }, et)
    };

    for et in [-900.0, -300.0, 500.0, 999.0] {
        let (emb, earth, moon, sun) = (st(0, et), st(1, et), st(2, et), st(3, et));
        let got = spk.state(301, 399, et).unwrap();
        assert_close(got.0, sub(moon.0, earth.0), TOL, "moon-earth pos");
        assert_close(got.1, sub(moon.1, earth.1), TOL, "moon-earth vel");
        assert_eq!(spk.position(301, 399, et).unwrap(), got.0);

        // Sun relative to Earth: SUN/SSB − EMB/SSB − EARTH/EMB.
        let want = sub(sub(sun.0, emb.0), earth.0);
        let got = spk.position(10, 399, et).unwrap();
        assert_close(got, want, TOL, "sun-earth");
        let back = spk.position(399, 10, et).unwrap();
        assert_close(back, [-want[0], -want[1], -want[2]], TOL, "earth-sun");

        assert_close(spk.position(3, 0, et).unwrap(), emb.0, TOL, "emb-ssb");
        assert_close(
            spk.position(0, 3, et).unwrap(),
            [-emb.0[0], -emb.0[1], -emb.0[2]],
            TOL,
            "ssb-emb",
        );
    }
    assert_eq!(spk.state(301, 301, 0.0).unwrap(), ([0.0; 3], [0.0; 3]));
}

#[test]
fn later_segments_take_precedence() {
    let specs = segments();
    let spk = Spk::from_bytes(build_spk(&specs, false, 25)).unwrap();
    assert_eq!(spk.find_segment(301, 100.0), Some(5));
    assert_eq!(spk.find_segment(301, 500.0), Some(2));
    assert_eq!(spk.find_segment(301, 1500.0), Some(4));
    // Boundary 1000 is covered by both 2 and 4; the later one wins.
    assert_eq!(spk.find_segment(301, 1000.0), Some(4));

    let p = spk.position(301, 3, 100.0).unwrap();
    assert_eq!(p, [11.0, 22.0, 33.0]);
    let (_, v) = spk.state(301, 3, 100.0).unwrap();
    assert_eq!(v, [0.0; 3]);
    let p = spk.position(301, 3, 1500.0).unwrap();
    assert_close(p, expect(&specs[4].records[0], 3, 1500.0).0, TOL, "moon-b");
}

#[test]
fn reports_coverage_and_path_errors() {
    let spk = Spk::from_bytes(build_spk(&segments(), false, 25)).unwrap();
    // Moon is covered at 1500 but the Earth is not.
    match spk.position(301, 399, 1500.0) {
        Err(SpkError::NoCoverage { body: 399, et }) => assert_eq!(et, 1500.0),
        other => panic!("expected NoCoverage for 399, got {other:?}"),
    }
    assert!(matches!(
        spk.position(301, 399, 5000.0),
        Err(SpkError::NoCoverage { body: 301, .. })
    ));
    assert!(matches!(
        spk.position(12345, 399, 0.0),
        Err(SpkError::NoPath {
            target: 12345,
            center: 399,
            ..
        })
    ));
    assert!(matches!(
        spk.position(499, 0, 0.0),
        Err(SpkError::UnsupportedType {
            segment: 6,
            data_type: 13
        })
    ));
    assert!(matches!(
        spk.position(599, 399, 0.0),
        Err(SpkError::UnsupportedFrame {
            segment: 7,
            frame: 17
        })
    ));
}

#[test]
fn byte_orders_agree() {
    let specs = segments();
    let le = Spk::from_bytes(build_spk(&specs, false, 2)).unwrap();
    let be = Spk::from_bytes(build_spk(&specs, true, 25)).unwrap();
    for et in [-999.0, -1.0, 0.0, 123.456, 777.0] {
        assert_eq!(
            le.state(10, 399, et).unwrap(),
            be.state(10, 399, et).unwrap()
        );
        assert_eq!(
            le.state(301, 399, et).unwrap(),
            be.state(301, 399, et).unwrap()
        );
    }
}

#[test]
fn reads_records_on_demand_and_caches_them() {
    let specs = segments();
    let bytes = build_spk(&specs, false, 3);
    let (reader, count) = CountingReader::new(Cursor::new(bytes));
    let spk = Spk::from_reader(reader).unwrap();
    // File record plus three (summary, name) record pairs.
    assert_eq!(count.load(Ordering::Relaxed), 1024 * 7);

    let before = count.load(Ordering::Relaxed);
    spk.segment_state(0, -100.0).unwrap();
    // Directory (4 words) plus one record of 2 + 3·4 words.
    let rsize = 2 + 3 * 4;
    assert_eq!(count.load(Ordering::Relaxed) - before, (4 + rsize) * 8);

    let before = count.load(Ordering::Relaxed);
    for et in [-100.0, -200.0, -999.0, -0.5] {
        spk.segment_state(0, et).unwrap();
    }
    assert_eq!(count.load(Ordering::Relaxed), before, "same record: cached");

    spk.segment_state(0, 100.0).unwrap();
    assert_eq!(
        count.load(Ordering::Relaxed) - before,
        rsize * 8,
        "second record: data only, directory cached"
    );
}

#[test]
fn borrows_an_in_memory_slice() {
    let bytes = build_spk(&segments(), false, 25);
    let slice: &[u8] = &bytes;
    let spk = Spk::from_bytes(slice).unwrap();
    let owned = Spk::from_bytes(bytes.clone()).unwrap();
    assert_eq!(
        spk.state(301, 399, 42.0).unwrap(),
        owned.state(301, 399, 42.0).unwrap()
    );
}

#[test]
fn spk_is_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Spk<'static>>();

    let spk = std::sync::Arc::new(Spk::from_bytes(build_spk(&segments(), false, 25)).unwrap());
    let want = spk.position(10, 399, 250.0).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let spk = spk.clone();
            std::thread::spawn(move || spk.position(10, 399, 250.0).unwrap())
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), want);
    }
}

#[test]
fn rejects_damaged_files() {
    let good = build_spk(&segments(), false, 3);

    let mut ftp = good.clone();
    ftp[699 + 9] = b'\r'; // "\n" mangled, as a text-mode transfer would
    assert!(matches!(Spk::from_bytes(ftp), Err(SpkError::Corrupt(_))));

    let mut ck = good.clone();
    ck[0..8].copy_from_slice(b"DAF/CK  ");
    assert!(matches!(
        Spk::from_bytes(ck),
        Err(SpkError::NotSpk { nd: 2, ni: 6, .. })
    ));

    let mut junk = good.clone();
    junk[0..8].copy_from_slice(b"GIF89a  ");
    assert!(matches!(Spk::from_bytes(junk), Err(SpkError::NotDaf)));

    // Cut inside the summary records: opening fails.
    match Spk::from_bytes(good[..1024 * 3].to_vec()) {
        Err(SpkError::Io(e)) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
        other => panic!("expected EOF, got {other:?}"),
    }

    // Cut inside the data: opening succeeds, evaluation reports EOF.
    let cut = Spk::from_bytes(good[..1024 * 7 + 64].to_vec()).unwrap();
    match cut.position(10, 0, 0.0) {
        Err(SpkError::Io(e)) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
        other => panic!("expected EOF, got {other:?}"),
    }

    // A segment directory inconsistent with the segment length.
    let mut bad_dir = segments();
    bad_dir[3].records.push(record(0.0, 1.0, 3, 9.0));
    bad_dir[3].intlen = 0.0;
    let spk = Spk::from_bytes(build_spk(&bad_dir, false, 25)).unwrap();
    assert!(matches!(
        spk.position(10, 0, 0.0),
        Err(SpkError::Corrupt(_))
    ));
}
