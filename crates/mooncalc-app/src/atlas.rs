//! City search and nearest-city lookup over the embedded GeoNames atlas
//! (`mooncalc_atlas`, CC BY 4.0; show [`mooncalc_atlas::ATTRIBUTION`]).

use mooncalc_atlas::Atlas;

use crate::dto::{ApiError, City, CitySearchParams, Location, NearestCity, NearestCityParams};

/// Default and maximum number of search results.
pub const CITY_LIMIT: (u32, u32) = (20, 100);

impl From<&mooncalc_atlas::City> for City {
    fn from(c: &mooncalc_atlas::City) -> Self {
        Self {
            name: c.name.into(),
            admin1: (!c.admin1.is_empty()).then(|| c.admin1.into()),
            country: c.country_name.into(),
            country_code: c.country_code.into(),
            location: Location {
                lat_deg: c.lat_deg,
                lon_deg: c.lon_deg,
                elevation_m: c.elevation_m,
            },
            time_zone: (!c.timezone.is_empty()).then(|| c.timezone.into()),
            std_offset_hours: c.std_offset_hours,
            dst_offset_hours: c.dst_offset_hours,
            population: (c.population > 0).then_some(c.population),
        }
    }
}

/// Search by name, case- and diacritic-insensitively; `City, Country`
/// narrows. Ranked by match quality, then population.
pub fn search_cities(p: &CitySearchParams) -> Vec<City> {
    let limit = p.limit.unwrap_or(CITY_LIMIT.0).clamp(1, CITY_LIMIT.1) as usize;
    Atlas::builtin()
        .search(p.query.trim(), limit)
        .into_iter()
        .map(City::from)
        .collect()
}

/// The atlas city nearest to a point.
pub fn nearest_city(p: &NearestCityParams) -> Result<Option<NearestCity>, ApiError> {
    if !(-90.0..=90.0).contains(&p.lat_deg) || !p.lon_deg.is_finite() {
        return Err(ApiError::invalid_params(
            "latDeg must be between -90 and 90 and lonDeg finite",
        ));
    }
    Ok(Atlas::builtin()
        .nearest_km(p.lat_deg, p.lon_deg)
        .map(|(c, d)| NearestCity {
            city: c.into(),
            distance_km: d,
        }))
}

/// `Name, Country` of the city nearest a point, with the distance when it
/// is more than 25 km away (`Majuro, Marshall Islands, 1450 km`), for labels.
pub fn nearest_label(lat_deg: f64, lon_deg: f64) -> Option<String> {
    Atlas::builtin()
        .nearest_km(lat_deg, lon_deg)
        .map(|(c, km)| {
            if km > 25.0 {
                format!("{}, {}, {km:.0} km", c.name, c.country_name)
            } else {
                format!("{}, {}", c.name, c.country_name)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_nearest() {
        let r = search_cities(&CitySearchParams {
            query: "Mecca".into(),
            limit: Some(3),
        });
        assert_eq!(r[0].country_code, "SA");
        assert_eq!(r[0].time_zone.as_deref(), Some("Asia/Riyadh"));
        assert_eq!(r[0].std_offset_hours, 3.0);
        assert!(r.len() <= 3);
        assert!(
            search_cities(&CitySearchParams {
                query: "  ".into(),
                limit: None
            })
            .is_empty()
        );
        let n = nearest_city(&NearestCityParams {
            lat_deg: 51.5,
            lon_deg: -0.12,
        })
        .unwrap()
        .unwrap();
        assert_eq!(n.city.country_code, "GB");
        assert!(n.distance_km < 20.0);
        assert!(
            nearest_city(&NearestCityParams {
                lat_deg: 95.0,
                lon_deg: 0.0
            })
            .is_err()
        );
    }
}
