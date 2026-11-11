//! The DE440 apparent-place [`Ephemeris`](mooncalc_core::Ephemeris)
//! backend. **Not implemented yet** (wave 2): it depends on the IAU 2006
//! precession and IAU 2000A/B nutation in `mooncalc-core`, written in
//! parallel with this crate.
//!
//! # Intended pipeline
//!
//! `De440Ephemeris` will own an `Spk<'static>` (opened from a user-chosen
//! `de440.bsp`/`de440s.bsp`, or from bytes in a WASM build) and implement the
//! `Ephemeris` trait. For a TT Julian Day `jd_tt`:
//!
//! 1. **Time.** `et = time::jd_tt_to_et(jd_tt)` (TT → TDB, then seconds past
//!    J2000).
//! 2. **Geometric vectors.** From [`Spk::state`](crate::Spk::state), the
//!    barycentric state of the Earth `E(t)` (399 relative to 0) and of the
//!    body `B(t)` (Sun 10 or Moon 301 relative to 0), in km, ICRF.
//! 3. **Light time.** Iterate `τ = |B(t − τ) − E(t)| / c` (two or three
//!    passes converge to well under a microsecond), giving the astrometric
//!    vector `p = B(t − τ) − E(t)`. The Sun also gets the relativistic
//!    gravitational deflection term only if needed for consistency with the
//!    Meeus backend; it is below 0.002″ near the Sun's own position and can
//!    be omitted (Urban & Seidelmann, *Explanatory Supplement*, 3rd ed.,
//!    §7.2.3).
//! 4. **Stellar aberration.** Apply the relativistic annual aberration with
//!    the Earth's barycentric velocity `Ė(t)` (*Explanatory Supplement*
//!    3rd ed., eq. 7.40; Kaplan, USNO Circular 179, §2.3). The Moon is
//!    geocentric, so its aberration reduces to the light-time correction
//!    with the Earth–Moon relative motion already in step 3 (the classical
//!    treatment Meeus uses); apply annual aberration to the Sun.
//! 5. **ICRF → true of date.** Rotate by the core's frame bias `B`,
//!    precession `P` (IAU 2006) and nutation `N` matrices:
//!    `r_tod = N · P · B · p` (IERS Conventions 2010, ch. 5).
//! 6. **Output.** Convert `r_tod` to [`Equatorial`](mooncalc_core::Equatorial)
//!    (right ascension, declination, distance in km); rotate by the true
//!    obliquity `ε = ε_A + Δε` about the x-axis for
//!    [`Ecliptic`](mooncalc_core::Ecliptic).
//!
//! `valid_range` comes from
//! [`Spk::common_coverage`](crate::Spk::common_coverage) over the Sun, the
//! Earth, the Moon and the Earth–Moon barycentre, converted with
//! `time::et_to_jd_tt`.
