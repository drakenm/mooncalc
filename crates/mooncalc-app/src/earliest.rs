//! Earliest sighting: for the new Moon nearest a date, the first evening on
//! which each zone of a criterion is reached anywhere on the world grid, and
//! where (`mooncalc_maps::engine::EarliestScan`).
//!
//! [`EarliestJob`] exposes the scan date by date so native callers can
//! evaluate each evening's [`MapJob`] in parallel (and WASM in workers);
//! [`earliest_sighting`] runs it all on the calling thread.

use mooncalc_core::Ephemeris;
use mooncalc_core::events::CalendarDate;
use mooncalc_core::time::Instant;
use mooncalc_maps::engine::{EarliestScan, conjunction_near, scan_dates};
use mooncalc_visibility::Criterion;

use crate::atlas::nearest_label;
use crate::criteria;
use crate::dto::{
    ApiError, Bounds, CivilDate, Crescent, EarliestParams, EarliestResult, EarliestZone,
    FirstSightingDto, MapParams, MapTiming,
};
use crate::map::{MapJob, MapOutput};
use crate::validate;

/// Default grid spacing of the scan, degrees.
pub const DEFAULT_RESOLUTION_DEG: f64 = 2.0;
/// Default number of evenings.
pub const DEFAULT_EVENINGS: u32 = 4;

/// The scan state.
pub struct EarliestJob {
    params: EarliestParams,
    criterion: &'static dyn Criterion,
    resolution_deg: f64,
    conjunction: Instant,
    dates: Vec<CalendarDate>,
    scanned: Vec<CalendarDate>,
    scan: EarliestScan,
}

impl EarliestJob {
    /// Validate and find the conjunction nearest noon UT of `params.date`.
    pub fn prepare(eph: &dyn Ephemeris, params: &EarliestParams) -> Result<Self, ApiError> {
        validate::date(params.date)?;
        let criterion = criteria::get(&params.criterion_id)?;
        let resolution_deg = params.resolution_deg.unwrap_or(DEFAULT_RESOLUTION_DEG);
        if !(0.25..=10.0).contains(&resolution_deg) {
            return Err(ApiError::invalid_params(
                "resolutionDeg must be between 0.25 and 10",
            ));
        }
        let evenings = params.evenings.unwrap_or(DEFAULT_EVENINGS);
        if !(1..=10).contains(&evenings) {
            return Err(ApiError::invalid_params(
                "evenings must be between 1 and 10",
            ));
        }
        let d = CalendarDate::from(params.date);
        let noon = Instant::from_jd_ut(d.jd0() + 0.5);
        crate::ephemeris::check_covers(eph, noon.jd_tt() - 32.0, noon.jd_tt() + 45.0)?;
        let conjunction = conjunction_near(eph, noon);
        Ok(Self {
            params: params.clone(),
            criterion,
            resolution_deg,
            conjunction,
            dates: scan_dates(conjunction, evenings),
            scanned: Vec::new(),
            scan: EarliestScan::new(criterion),
        })
    }

    pub fn conjunction(&self) -> Instant {
        self.conjunction
    }

    /// Every evening the scan may cover, in order.
    pub fn dates(&self) -> &[CalendarDate] {
        &self.dates
    }

    /// The map request for one evening (world grid at the scan resolution).
    pub fn map_params(&self, date: CalendarDate) -> MapParams {
        MapParams {
            date: date.into(),
            criterion_id: self.criterion.id().into(),
            crescent: Crescent::Evening,
            resolution_deg: self.resolution_deg,
            bounds: Some(Bounds::WORLD),
            ephemeris_id: self.params.ephemeris_id.clone(),
            include_values: false,
        }
    }

    /// Fold in one evaluated evening (dates in increasing order).
    pub fn accumulate(&mut self, out: &MapOutput) {
        let date = CalendarDate::from(out.params.date);
        self.scan.accumulate(date, &out.grid, &out.field);
        self.scanned.push(date);
    }

    /// Whether every zone has been reached.
    pub fn is_complete(&self) -> bool {
        self.scan.is_complete()
    }

    /// The wire result.
    pub fn result(&self, timing: MapTiming) -> EarliestResult {
        let zones = self.criterion.zones();
        EarliestResult {
            criterion_id: self.criterion.id().into(),
            criterion_name: self.criterion.name().into(),
            conjunction: self.conjunction.into(),
            dates_scanned: self.scanned.iter().map(|&d| CivilDate::from(d)).collect(),
            zones: self
                .scan
                .results()
                .iter()
                .enumerate()
                .map(|(i, f)| EarliestZone {
                    zone: i as u8,
                    code: zones[i].code.into(),
                    description: zones[i].description.into(),
                    rgb: zones[i].rgb,
                    first: f.map(|f| FirstSightingDto {
                        date: f.date.into(),
                        lat_deg: f.lat_deg,
                        lon_deg: f.lon_deg,
                        value: f.value.map(f64::from),
                        cells: f.cells as u32,
                        nearest_city: nearest_label(f.lat_deg, f.lon_deg),
                    }),
                })
                .collect(),
            resolution_deg: self.resolution_deg,
            timing,
        }
    }
}

/// Run the whole scan on the calling thread (no timing).
pub fn earliest_sighting(
    eph: &dyn Ephemeris,
    params: &EarliestParams,
) -> Result<EarliestResult, ApiError> {
    let mut job = EarliestJob::prepare(eph, params)?;
    for date in job.dates().to_vec() {
        let map = MapJob::prepare(eph, &job.map_params(date))?;
        job.accumulate(&map.evaluate());
        if job.is_complete() {
            break;
        }
    }
    Ok(job.result(MapTiming {
        threads: 1,
        ..MapTiming::default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::meeus;

    #[test]
    fn march_2024_zone_a_first_in_the_americas() {
        // New Moon 2024 March 10 09:00 UT: zone A is first reached on the
        // evening of March 10, in the Americas (HMNAO's map for that date).
        let eph = meeus(false);
        let r = earliest_sighting(
            &*eph,
            &EarliestParams {
                date: CivilDate {
                    year: 2024,
                    month: 3,
                    day: 12,
                },
                criterion_id: "yallop".into(),
                resolution_deg: Some(6.0),
                evenings: Some(4),
                ephemeris_id: None,
            },
        )
        .unwrap();
        assert_eq!((r.conjunction.day, r.conjunction.hour), (10, 9));
        assert_eq!(r.zones.len(), 5);
        let a = r.zones[0].first.as_ref().unwrap();
        assert_eq!(
            a.date,
            CivilDate {
                year: 2024,
                month: 3,
                day: 10
            }
        );
        assert!(a.lon_deg < -30.0, "{a:?}");
        assert!(a.nearest_city.is_some());
        assert_eq!(r.dates_scanned[0].day, 9);
        assert!(r.dates_scanned.len() <= 4);
    }

    #[test]
    fn rejects_bad_params() {
        let eph = meeus(false);
        let p = EarliestParams {
            date: CivilDate {
                year: 2024,
                month: 3,
                day: 12,
            },
            criterion_id: "yallop".into(),
            resolution_deg: Some(0.1),
            evenings: None,
            ephemeris_id: None,
        };
        assert!(EarliestJob::prepare(&*eph, &p).is_err());
        let p = EarliestParams {
            resolution_deg: None,
            evenings: Some(0),
            ..p
        };
        assert!(EarliestJob::prepare(&*eph, &p).is_err());
    }
}
