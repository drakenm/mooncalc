//! Credits shown on the About tab.
//!
//! **Extension point:** when you add data, an algorithm source or a library
//! that needs crediting, append an entry to [`ATTRIBUTIONS`].

/// One credited work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribution {
    /// What is credited, e.g. "GeoNames".
    pub name: &'static str,
    /// What MoonCalc uses it for.
    pub usage: &'static str,
    /// Licence or citation terms, e.g. "CC BY 4.0".
    pub license: &'static str,
    /// Link to the source, if any.
    pub url: Option<&'static str>,
}

/// Every credit, in display order.
pub const ATTRIBUTIONS: &[Attribution] = &[
    Attribution {
        name: "MoonCalc 6.0 by Dr. Monzur Ahmed",
        usage: "The original crescent-visibility program that inspired this \
                application. MoonCalc-rs is an independent reimplementation \
                from published algorithms and contains none of its code.",
        license: "Inspiration only",
        url: None,
    },
    Attribution {
        name: "Jean Meeus, Astronomical Algorithms (2nd ed., 1998)",
        usage: "Time scales, calendar conversions, and the default Sun (VSOP87) \
                and Moon (ELP-2000/82) theories.",
        license: "Published algorithms",
        url: None,
    },
    Attribution {
        name: "B. D. Yallop (1997), NAO Technical Note No. 69",
        usage: "\"A Method for Predicting the First Sighting of the New \
                Crescent Moon\": the q-test and zones A–F.",
        license: "Published method",
        url: None,
    },
    Attribution {
        name: "M. Sh. Odeh (2004), Experimental Astronomy 18, 39–64",
        usage: "\"New Criterion for Lunar Crescent Visibility\": the V-test.",
        license: "Published method",
        url: None,
    },
    Attribution {
        name: "JPL DE440 (NASA / Jet Propulsion Laboratory)",
        usage: "Optional high-precision planetary and lunar ephemeris \
                (builds with the de440 feature; file supplied by the user).",
        license: "U.S. Government work",
        url: Some("https://ssd.jpl.nasa.gov/planets/eph_export.html"),
    },
    Attribution {
        name: "Natural Earth",
        usage: "Coastlines and country borders on the world maps.",
        license: "Public domain",
        url: Some("https://www.naturalearthdata.com"),
    },
    Attribution {
        name: "GeoNames",
        usage: "The city atlas used by the location search.",
        license: "CC BY 4.0",
        url: Some("https://www.geonames.org"),
    },
    Attribution {
        name: "egui / eframe",
        usage: "The user-interface toolkit.",
        license: "MIT OR Apache-2.0",
        url: Some("https://github.com/emilk/egui"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_credits_are_present() {
        for needle in [
            "MoonCalc 6.0",
            "Meeus",
            "Yallop",
            "Odeh",
            "DE440",
            "Natural Earth",
            "GeoNames",
            "egui",
        ] {
            assert!(
                ATTRIBUTIONS.iter().any(|a| a.name.contains(needle)),
                "missing credit for {needle}"
            );
        }
        let geonames = ATTRIBUTIONS.iter().find(|a| a.name == "GeoNames").unwrap();
        assert_eq!(geonames.license, "CC BY 4.0");
        assert_eq!(geonames.url, Some("https://www.geonames.org"));
    }
}
