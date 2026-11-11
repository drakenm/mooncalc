//! Umm al-Qura against published reference dates: archived Umm al-Qura
//! newspaper issues (1343-1355 AH), the KFUPM comparison calendar
//! (1356-1411 AH) and the official Umm al-Qura website (1431-1500 AH), as
//! collected in the hijridate test fixtures (MIT licence).

mod data;

use data::umalqura_reference::UMALQURA_REFERENCE;
use mooncalc_hijri::civil::{CivilCalendar, CivilDate};
use mooncalc_hijri::{HijriCalendar, HijriDate, UMM_AL_QURA, UMM_AL_QURA_ICU};

fn jdn(y: i32, m: u8, d: u8) -> i64 {
    CivilDate::new(y, m, d)
        .to_jdn(CivilCalendar::Gregorian)
        .unwrap()
}

#[test]
fn official_table_matches_every_reference_date() {
    assert!(UMALQURA_REFERENCE.len() > 1800);
    for &(hy, hm, hd, gy, gm, gd) in UMALQURA_REFERENCE.iter() {
        let h = HijriDate::new(hy, hm, hd);
        let j = jdn(gy, gm, gd);
        assert_eq!(UMM_AL_QURA.to_hijri(j), Some(h), "{gy}-{gm}-{gd}");
        assert_eq!(UMM_AL_QURA.from_hijri(h), Some(j), "{h}");
    }
}

#[test]
fn icu_table_matches_references_for_1420_to_1450() {
    let mut checked = 0;
    for &(hy, hm, hd, gy, gm, gd) in UMALQURA_REFERENCE.iter() {
        if (1420..=1450).contains(&hy) {
            let h = HijriDate::new(hy, hm, hd);
            assert_eq!(UMM_AL_QURA_ICU.to_hijri(jdn(gy, gm, gd)), Some(h));
            checked += 1;
        }
    }
    assert!(checked > 200);
}
