//! MoonCalc application services, shared by every frontend.
//!
//! High-level operations from a request DTO to a response DTO ([`dto`]),
//! with no frontend types: the Tauri shell exposes them as commands, the
//! planned WASM bindings will export them to TypeScript, and native
//! frontends may call them directly.
//!
//! | Service | Operation |
//! | --- | --- |
//! | [`info`] | [`info::app_info`]: version, runtime, ephemeris backends |
//! | [`ephemeris`] | [`ephemeris::Ephemerides`]: Meeus (truncated/full), DE440 (feature `de440`) |
//! | [`criteria`] | [`criteria::list`]: criteria with legends and references |
//! | [`map`] | [`map::MapJob`]: prepare → evaluate row blocks → assemble → [`dto::MapResult`]; PNG export |
//! | [`report`] | [`report::location_report`]: day events, evening and morning crescents under every criterion |
//! | [`atlas`] | [`atlas::search_cities`], [`atlas::nearest_city`] |
//! | [`earliest`] | [`earliest::EarliestJob`]: earliest-sighting scan after a conjunction |
//! | [`basemap`] | [`basemap::basemap`]: simplified coastlines and borders |
//!
//! # Portability
//!
//! The crate builds for `wasm32-unknown-unknown` (default features): no
//! threads, clocks or file system in its default paths. Long computations
//! are split so the caller can parallelise them ([`map::MapJob::evaluate_rows`])
//! and time them ([`dto::MapTiming`] is filled in by the caller). The only
//! file access is [`ephemeris::de440_open`] (feature `de440`, non-wasm).

pub mod atlas;
pub mod basemap;
pub mod codec;
pub mod criteria;
pub mod dto;
pub mod earliest;
pub mod ephemeris;
pub mod info;
pub mod map;
pub mod report;
pub mod sky;
pub mod trajectory;
pub mod validate;

#[cfg(test)]
mod wire_tests {
    //! The JSON shapes the TypeScript side sends and receives.

    use crate::dto::*;
    use serde_json::json;

    #[test]
    fn map_params_accept_the_typescript_shape() {
        let p: MapParams = serde_json::from_value(json!({
            "date": { "year": 2026, "month": 11, "day": 10 },
            "criterionId": "yallop",
            "crescent": "morning",
            "resolutionDeg": 0.5
        }))
        .unwrap();
        assert_eq!(p.crescent, Crescent::Morning);
        assert_eq!(p.ephemeris_id, None);
        assert_eq!(p.bounds, None);
        assert!(p.include_values);
    }

    #[test]
    fn results_serialize_in_camel_case() {
        let v = serde_json::to_value(Location {
            lat_deg: 1.0,
            lon_deg: 2.0,
            elevation_m: 3.0,
        })
        .unwrap();
        assert_eq!(
            v,
            json!({ "latDeg": 1.0, "lonDeg": 2.0, "elevationM": 3.0 })
        );
        let e = serde_json::to_value(ApiError::invalid_params("x")).unwrap();
        assert_eq!(e, json!({ "code": "invalid-params", "message": "x" }));

        let eph = crate::ephemeris::meeus(false);
        let r = crate::map::compute_map(
            &*eph,
            &MapParams {
                date: CivilDate {
                    year: 2026,
                    month: 11,
                    day: 10,
                },
                criterion_id: "odeh".into(),
                crescent: Crescent::Evening,
                resolution_deg: 10.0,
                bounds: None,
                ephemeris_id: None,
                include_values: true,
            },
        )
        .unwrap();
        let v = serde_json::to_value(&r).unwrap();
        for key in [
            "criterionId",
            "criterionName",
            "date",
            "crescent",
            "grid",
            "cells",
            "noData",
            "legend",
            "values",
            "thresholds",
            "zoneCounts",
            "statusCounts",
            "conjunction",
            "evaluationTime",
            "best",
            "ephemeris",
            "timing",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert_eq!(v["crescent"], "evening");
        assert_eq!(v["grid"]["latMinDeg"], -90.0);
        assert!(v["statusCounts"].get("moonSetsFirst").is_some());
        assert!(v["timing"].get("evaluateMs").is_some());

        let b: BasemapParams = serde_json::from_value(json!({ "scale": "50m" })).unwrap();
        assert_eq!(b.scale, BasemapScale::Ne50m);
        assert_eq!(
            serde_json::to_value(BasemapScale::Ne110m).unwrap(),
            json!("110m")
        );
    }
}
