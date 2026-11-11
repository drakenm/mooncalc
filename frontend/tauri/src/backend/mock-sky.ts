// Mock Sky, Trajectory and Moon operations for `npm run dev` in a browser.
//
// A deliberately small astronomical model so the tabs show plausible skies
// without the Rust core: the low-precision Sun and Moon of the Astronomical
// Almanac (about 0.01° for the Sun, 0.3° for the Moon), no ΔT, nutation or
// precession, Bennett refraction, horizon events found by one-minute
// scanning, optical libration only (Meeus ch. 53, mean elements) and a few
// bright stars at J2000. The desktop app uses the full Meeus/ELP or DE440
// computation in mooncalc-app instead.

import { BackendError } from './backend';
import type { CrescentGeometry, Location, UtDateTime, Verdict } from './types';
import type {
  BodyDayEvents,
  LibrationTrack,
  MoonFeatures,
  MoonPhysicalDto,
  MoonView,
  MoonViewParams,
  PhaseCode,
  PhaseEvent,
  SkyApi,
  SkySample,
  SkyScene,
  SkySceneParams,
  SkyStars,
  SkyStarsParams,
  StarDto,
  TrackSample,
  TrajectoryDay,
  TrajectoryParams,
} from './types-sky';

const D = Math.PI / 180;
const sin = (d: number) => Math.sin(d * D);
const cos = (d: number) => Math.cos(d * D);
const norm360 = (d: number) => ((d % 360) + 360) % 360;
const norm180 = (d: number) => norm360(d + 180) - 180;
const atan2d = (y: number, x: number) => Math.atan2(y, x) / D;
const asind = (x: number) => Math.asin(Math.max(-1, Math.min(1, x))) / D;
const EARTH_RADIUS_KM = 6378.14;
const AU_KM = 149_597_870.7;

// --- Time ----------------------------------------------------------------

export function utToJd(t: UtDateTime): number {
  return Date.UTC(t.year, t.month - 1, t.day, t.hour, t.minute, 0, t.second * 1000) / 86_400_000 + 2_440_587.5;
}

function jdToUt(jd: number): UtDateTime {
  const d = new Date(Math.round((jd - 2_440_587.5) * 86_400_000));
  return {
    year: d.getUTCFullYear(),
    month: d.getUTCMonth() + 1,
    day: d.getUTCDate(),
    hour: d.getUTCHours(),
    minute: d.getUTCMinutes(),
    second: d.getUTCSeconds(),
  };
}

// --- Positions -------------------------------------------------------------

interface Eq {
  ra: number; // degrees
  dec: number;
  distKm: number;
}

interface Body {
  lon: number; // ecliptic, degrees
  lat: number;
  distKm: number;
}

function sunEcl(jd: number): Body {
  const T = (jd - 2_451_545) / 36_525;
  const L = 280.46 + 36_000.77 * T;
  const g = 357.528 + 35_999.05 * T;
  const lon = L + 1.915 * sin(g) + 0.02 * sin(2 * g);
  const r = 1.00014 - 0.01671 * cos(g) - 0.00014 * cos(2 * g);
  return { lon: norm360(lon), lat: 0, distKm: r * AU_KM };
}

function moonEcl(jd: number): Body {
  const T = (jd - 2_451_545) / 36_525;
  const lon =
    218.32 + 481_267.881 * T +
    6.29 * sin(134.9 + 477_198.85 * T) - 1.27 * sin(259.2 - 413_335.38 * T) +
    0.66 * sin(235.7 + 890_534.23 * T) + 0.21 * sin(269.9 + 954_397.7 * T) -
    0.19 * sin(357.5 + 35_999.05 * T) - 0.11 * sin(186.6 + 966_404.05 * T);
  const lat =
    5.13 * sin(93.3 + 483_202.03 * T) + 0.28 * sin(228.2 + 960_400.87 * T) -
    0.28 * sin(318.3 + 6_003.18 * T) - 0.17 * sin(217.6 - 407_332.2 * T);
  const par =
    0.9508 + 0.0518 * cos(134.9 + 477_198.85 * T) + 0.0095 * cos(259.2 - 413_335.38 * T) +
    0.0078 * cos(235.7 + 890_534.23 * T) + 0.0028 * cos(269.9 + 954_397.7 * T);
  return { lon: norm360(lon), lat, distKm: EARTH_RADIUS_KM / sin(par) };
}

function obliquity(jd: number): number {
  return 23.439 - 0.013 * ((jd - 2_451_545) / 36_525);
}

function toEq(b: Body, eps: number): Eq {
  const x = cos(b.lat) * cos(b.lon);
  const y = cos(eps) * cos(b.lat) * sin(b.lon) - sin(eps) * sin(b.lat);
  const z = sin(eps) * cos(b.lat) * sin(b.lon) + cos(eps) * sin(b.lat);
  return { ra: norm360(atan2d(y, x)), dec: asind(z), distKm: b.distKm };
}

function lstDeg(jd: number, lonDeg: number): number {
  return norm360(280.46061837 + 360.98564736629 * (jd - 2_451_545) + lonDeg);
}

/** Topocentric place (spherical Earth, sea level). */
function topocentric(e: Eq, latDeg: number, lst: number): Eq {
  const v = [e.distKm * cos(e.dec) * cos(e.ra), e.distKm * cos(e.dec) * sin(e.ra), e.distKm * sin(e.dec)];
  const o = [EARTH_RADIUS_KM * cos(latDeg) * cos(lst), EARTH_RADIUS_KM * cos(latDeg) * sin(lst), EARTH_RADIUS_KM * sin(latDeg)];
  const t = [v[0]! - o[0]!, v[1]! - o[1]!, v[2]! - o[2]!];
  const r = Math.hypot(t[0]!, t[1]!, t[2]!);
  return { ra: norm360(atan2d(t[1]!, t[0]!)), dec: asind(t[2]! / r), distKm: r };
}

function horizontal(e: Eq, latDeg: number, lst: number): { alt: number; az: number; ha: number } {
  const ha = norm180(lst - e.ra);
  const alt = asind(sin(latDeg) * sin(e.dec) + cos(latDeg) * cos(e.dec) * cos(ha));
  const az = norm360(atan2d(-cos(e.dec) * sin(ha), sin(e.dec) * cos(latDeg) - cos(e.dec) * cos(ha) * sin(latDeg)));
  return { alt, az, ha };
}

/** Bennett's refraction, degrees, for an airless altitude. */
function refract(h: number): number {
  if (h < -1.5) return h;
  return h + 1 / Math.tan((h + 7.31 / (h + 4.4)) * D) / 60;
}

interface Snap {
  jd: number;
  lst: number;
  sun: Eq;
  moon: Eq;
  sunH: { alt: number; az: number; ha: number };
  moonH: { alt: number; az: number; ha: number };
  sunAirless: number;
  moonAirless: number;
  elong: number;
  k: number;
  chi: number;
  q: number;
  elongLon: number;
  moonGeo: Body;
  sunGeo: Body;
}

function snap(jd: number, loc: Location): Snap {
  const eps = obliquity(jd);
  const sunGeo = sunEcl(jd);
  const moonGeo = moonEcl(jd);
  const lst = lstDeg(jd, loc.lonDeg);
  const sun = toEq(sunGeo, eps);
  const moon = topocentric(toEq(moonGeo, eps), loc.latDeg, lst);
  const sh = horizontal(sun, loc.latDeg, lst);
  const mh = horizontal(moon, loc.latDeg, lst);
  const elong = Math.acos(sin(sun.dec) * sin(moon.dec) + cos(sun.dec) * cos(moon.dec) * cos(sun.ra - moon.ra)) / D;
  const i = atan2d(sun.distKm * sin(elong), moon.distKm - sun.distKm * cos(elong));
  const chi = norm360(
    atan2d(cos(sun.dec) * sin(sun.ra - moon.ra), sin(sun.dec) * cos(moon.dec) - cos(sun.dec) * sin(moon.dec) * cos(sun.ra - moon.ra)),
  );
  const q = atan2d(sin(mh.ha) * cos(loc.latDeg), sin(loc.latDeg) * cos(moon.dec) - cos(loc.latDeg) * sin(moon.dec) * cos(mh.ha));
  return {
    jd,
    lst,
    sun,
    moon,
    sunH: { ...sh, alt: refract(sh.alt) },
    moonH: { ...mh, alt: refract(mh.alt) },
    sunAirless: sh.alt,
    moonAirless: mh.alt,
    elong,
    k: (1 + cos(i)) / 2,
    chi,
    q,
    elongLon: norm360(moonGeo.lon - sunGeo.lon),
    moonGeo,
    sunGeo,
  };
}

const moonSdArcmin = (km: number) => (Math.asin(1737.4 / km) / D) * 60;
const sunSdArcmin = (km: number) => (959.63 / 60) * (AU_KM / km);

// --- Events ------------------------------------------------------------------

/** Times in [jd0, jd1] where f crosses zero, by one-minute scanning. */
function crossings(f: (jd: number) => number, jd0: number, jd1: number): { jd: number; rising: boolean }[] {
  const step = 1 / 1440;
  const out: { jd: number; rising: boolean }[] = [];
  let a = jd0;
  let fa = f(a);
  for (let b = jd0 + step; b <= jd1 + 1e-9; b += step) {
    const fb = f(b);
    if ((fa < 0) !== (fb < 0)) out.push({ jd: a + (step * fa) / (fa - fb), rising: fb > fa });
    a = b;
    fa = fb;
  }
  return out;
}

const SUN_HORIZON = -0.8333;
const moonHorizon = (s: Snap) => 0.7275 * (asind(EARTH_RADIUS_KM / s.moon.distKm)) - 0.5667;

function phasesBetween(jd0: number, jd1: number): PhaseEvent[] {
  const codes: PhaseCode[] = ['new', 'first-quarter', 'full', 'last-quarter'];
  const out: PhaseEvent[] = [];
  const e = (jd: number) => norm360(moonEcl(jd).lon - sunEcl(jd).lon);
  for (let i = 0; i < 4; i++) {
    const f = (jd: number) => norm180(e(jd) - i * 90);
    const step = 0.25;
    for (let a = jd0; a < jd1; a += step) {
      const fa = f(a);
      const fb = f(a + step);
      if (fa < 0 && fb >= 0 && fb - fa < 90) {
        let lo = a;
        let hi = a + step;
        for (let k = 0; k < 30; k++) {
          const m = (lo + hi) / 2;
          if (f(m) < 0) lo = m;
          else hi = m;
        }
        out.push({ phase: codes[i]!, time: jdToUt((lo + hi) / 2) });
      }
    }
  }
  return out.sort((a, b) => utToJd(a.time) - utToJd(b.time));
}

// --- Stars (a few of the brightest, J2000) --------------------------------------

const STARS: [string, string, number, number, number, number][] = [
  ['Sirius', 'α CMa', 101.287, -16.716, -1.46, 0.0],
  ['Canopus', 'α Car', 95.988, -52.696, -0.72, 0.15],
  ['Arcturus', 'α Boo', 213.915, 19.183, -0.04, 1.23],
  ['Vega', 'α Lyr', 279.235, 38.784, 0.03, 0.0],
  ['Capella', 'α Aur', 79.173, 45.998, 0.08, 0.8],
  ['Rigel', 'β Ori', 78.635, -8.202, 0.12, -0.03],
  ['Procyon', 'α CMi', 114.825, 5.225, 0.38, 0.42],
  ['Betelgeuse', 'α Ori', 88.793, 7.407, 0.5, 1.85],
  ['Achernar', 'α Eri', 24.429, -57.237, 0.46, -0.16],
  ['Altair', 'α Aql', 297.696, 8.868, 0.77, 0.22],
  ['Aldebaran', 'α Tau', 68.98, 16.509, 0.85, 1.54],
  ['Antares', 'α Sco', 247.352, -26.432, 0.96, 1.83],
  ['Spica', 'α Vir', 201.298, -11.161, 0.98, -0.23],
  ['Pollux', 'β Gem', 116.329, 28.026, 1.14, 1.0],
  ['Fomalhaut', 'α PsA', 344.413, -29.622, 1.16, 0.09],
  ['Deneb', 'α Cyg', 310.358, 45.28, 1.25, 0.09],
  ['Regulus', 'α Leo', 152.093, 11.967, 1.35, -0.11],
  ['Castor', 'α Gem', 113.65, 31.888, 1.58, 0.03],
  ['Bellatrix', 'γ Ori', 81.283, 6.35, 1.64, -0.22],
  ['Alnilam', 'ε Ori', 84.053, -1.202, 1.7, -0.19],
  ['Polaris', 'α UMi', 37.954, 89.264, 2.02, 0.6],
  ['Dubhe', 'α UMa', 165.932, 61.751, 1.79, 1.07],
  ['Alkaid', 'η UMa', 206.885, 49.313, 1.86, -0.19],
  ['Mirfak', 'α Per', 51.081, 49.861, 1.79, 0.48],
  ['Hamal', 'α Ari', 31.793, 23.462, 2.0, 1.15],
];

function stars(mag: number): StarDto[] {
  return STARS.filter((s) => s[4] <= mag).map(([name, label, ra, dec, vmag, bv], i) => ({
    hr: i + 1,
    name,
    label,
    vmag,
    bv,
    raDeg: ra,
    decDeg: dec,
    altDeg: null,
    azDeg: null,
  }));
}

// --- Crescent visibility (Yallop 1997, Odeh 2004) -------------------------------

function verdict(criterionId: string, arcv: number, widthArcmin: number): Verdict {
  const w = widthArcmin;
  const poly = -0.1018 * w ** 3 + 0.7319 * w ** 2 - 6.3226 * w;
  if (criterionId === 'odeh') {
    const v = arcv - (poly + 7.1651);
    const zones: [number, string, string, [number, number, number]][] = [
      [5.65, 'A', 'Visible by naked eye', [46, 160, 67]],
      [2, 'B', 'Visible by optical aid, could be seen by naked eye', [154, 209, 75]],
      [-0.96, 'C', 'Visible by optical aid only', [240, 140, 46]],
      [-Infinity, 'D', 'Not visible even with optical aid', [214, 60, 60]],
    ];
    const z = zones.findIndex((x) => v >= x[0]);
    return { zone: z, code: zones[z]![1], description: zones[z]![2], rgb: zones[z]![3], value: v };
  }
  const q = (arcv - (poly + 11.8371)) / 10;
  const zones: [number, string, string, [number, number, number]][] = [
    [0.216, 'A', 'Easily visible (ARCL ≥ 12°)', [46, 158, 63]],
    [-0.014, 'B', 'Visible under perfect conditions', [166, 216, 84]],
    [-0.16, 'C', 'May need optical aid to find the crescent', [255, 217, 47]],
    [-0.232, 'D', 'Will need optical aid to find the crescent', [252, 141, 89]],
    [-0.293, 'E', 'Not visible with a telescope (ARCL ≤ 8.5°)', [215, 48, 39]],
    [-Infinity, 'F', 'Not visible, below the Danjon limit (ARCL ≤ 8°)', [150, 150, 150]],
  ];
  const z = zones.findIndex((x) => q >= x[0]);
  return { zone: z, code: zones[z]![1], description: zones[z]![2], rgb: zones[z]![3], value: q };
}

function skySample(s: Snap): SkySample {
  const sd = moonSdArcmin(s.moon.distKm);
  return {
    sunAltDeg: s.sunH.alt,
    sunAzDeg: s.sunH.az,
    moonAltDeg: s.moonH.alt,
    moonAzDeg: s.moonH.az,
    sunRaDeg: s.sun.ra,
    sunDecDeg: s.sun.dec,
    moonRaDeg: s.moon.ra,
    moonDecDeg: s.moon.dec,
    lstDeg: s.lst,
    illuminatedFraction: s.k,
    brightLimbDeg: s.chi,
    parallacticDeg: s.q,
    limbZenithDeg: norm360(s.chi - s.q),
    moonSemiDiameterArcmin: sd,
    sunSemiDiameterArcmin: sunSdArcmin(s.sun.distKm),
    moonDistanceKm: s.moon.distKm,
    arclDeg: s.elong,
    arcvDeg: s.moonAirless - s.sunAirless,
    dazDeg: norm180(s.sunH.az - s.moonH.az),
    widthArcmin: sd * (1 - cos(s.elong)),
  };
}

function skyScene(p: SkySceneParams): SkyScene {
  const evening = p.crescent === 'evening';
  const noon = utToJd({ ...p.date, hour: 12, minute: 0, second: 0 }) - p.tzOffsetHours / 24;
  const loc = p.location;
  const sunEvents = crossings((jd) => snap(jd, loc).sunH.alt - SUN_HORIZON, evening ? noon : noon - 1, evening ? noon + 1 : noon);
  const sunEvent = evening ? sunEvents.find((e) => !e.rising)?.jd : sunEvents.filter((e) => e.rising).pop()?.jd;
  let moonEvent: number | undefined;
  if (sunEvent !== undefined) {
    const ev = crossings((jd) => {
      const s = snap(jd, loc);
      return s.moonH.alt - moonHorizon(s);
    }, sunEvent - 0.5, sunEvent + 0.5);
    moonEvent = evening
      ? ev.filter((e) => !e.rising && e.jd > sunEvent - 0.25).sort((a, b) => Math.abs(a.jd - sunEvent) - Math.abs(b.jd - sunEvent))[0]?.jd
      : ev.filter((e) => e.rising && e.jd < sunEvent + 0.25).sort((a, b) => Math.abs(a.jd - sunEvent) - Math.abs(b.jd - sunEvent))[0]?.jd;
  }
  const phases = phasesBetween(noon - 20, noon + 20);
  const conj = phases
    .filter((x) => x.phase === 'new')
    .map((x) => utToJd(x.time))
    .sort((a, b) => Math.abs(a - noon) - Math.abs(b - noon))[0];
  const lagOk = sunEvent !== undefined && moonEvent !== undefined && (evening ? moonEvent > sunEvent : moonEvent < sunEvent);
  const best = lagOk ? sunEvent! + (4 / 9) * (moonEvent! - sunEvent!) : undefined;
  const status = sunEvent === undefined ? 'midnight-sun' : !lagOk ? 'moon-sets-first' : conj !== undefined && (evening ? best! < conj : best! > conj) ? 'before-conjunction' : 'crescent';
  const pad = 10 / 1440;
  const start = sunEvent === undefined ? noon + 5 / 24 : lagOk ? Math.min(sunEvent, moonEvent!) - pad : evening ? sunEvent - pad : sunEvent - 2 / 24;
  const end = sunEvent === undefined ? noon + 9 / 24 : lagOk ? Math.max(sunEvent, moonEvent!) + pad : evening ? sunEvent + 2 / 24 : sunEvent + pad;
  const step = 2 / 1440;
  const s0 = start - 1 / 24;
  const n = Math.ceil((end + 1 / 24 - s0) / step);
  const samples = Array.from({ length: n + 1 }, (_, i) => skySample(snap(s0 + i * step, loc)));
  const criterionId = p.criterionId ?? 'yallop';
  let geometry: CrescentGeometry | null = null;
  let v: Verdict | null = null;
  if (best !== undefined && conj !== undefined) {
    const s = snap(best, loc);
    const x = skySample(s);
    geometry = {
      arclDeg: x.arclDeg,
      arclGeoDeg: x.arclDeg,
      arcvDeg: x.arcvDeg,
      arcvTopoDeg: x.arcvDeg,
      dazDeg: x.dazDeg,
      widthTopoArcmin: x.widthArcmin!,
      widthGeoArcmin: x.widthArcmin!,
      semiDiameterTopoArcmin: x.moonSemiDiameterArcmin,
      moonAltDeg: x.moonAltDeg,
      moonAltAirlessDeg: s.moonAirless,
      sunAltDeg: x.sunAltDeg,
      moonAzDeg: x.moonAzDeg,
      sunAzDeg: x.sunAzDeg,
      lagMinutes: Math.abs(moonEvent! - sunEvent!) * 1440,
      ageHours: (evening ? best - conj : conj - best) * 24,
      illuminatedFraction: x.illuminatedFraction,
      moonDistanceKm: x.moonDistanceKm,
    };
    if (status === 'crescent') v = verdict(criterionId, x.arcvDeg, x.widthArcmin!);
  }
  const ut = (jd: number | undefined) => (jd === undefined ? null : jdToUt(jd));
  return {
    location: loc,
    date: p.date,
    tzOffsetHours: p.tzOffsetHours,
    timeZone: p.timeZone ?? null,
    crescent: p.crescent,
    ephemeris: 'mock (low precision)',
    criterionId,
    criterionName: criterionId === 'odeh' ? 'Odeh (2004)' : 'Yallop (1997)',
    evaluationTime: 'Best time (sunset + 4/9 lag)',
    status,
    statusText: 'Mock backend: low-precision positions computed in the browser.',
    sunEvent: ut(sunEvent),
    moonEvent: ut(moonEvent),
    bestTime: ut(best),
    conjunction: ut(conj),
    start: jdToUt(start),
    end: jdToUt(end),
    defaultTime: jdToUt(best ?? sunEvent ?? start),
    geometry,
    verdict: v,
    samplesStart: jdToUt(s0),
    stepMinutes: 2,
    samples,
    stars: stars(p.magnitudeLimit ?? 3.5),
    obliquityDeg: obliquity(noon),
  };
}

function skyStars(p: SkyStarsParams): SkyStars {
  const jd = utToJd(p.instant);
  const loc = p.location ?? null;
  const lst = loc ? lstDeg(jd, loc.lonDeg) : null;
  return {
    instant: p.instant,
    lstDeg: lst,
    obliquityDeg: obliquity(jd),
    stars: stars(p.magnitudeLimit ?? 3.5).map((s) => {
      if (!loc || lst === null) return s;
      const h = horizontal({ ra: s.raDeg, dec: s.decDeg, distKm: 1 }, loc.latDeg, lst);
      return { ...s, altDeg: refract(h.alt), azDeg: h.az };
    }),
  };
}

function bodyEvents(jd0: number, loc: Location, moon: boolean): BodyDayEvents {
  const f = (jd: number) => {
    const s = snap(jd, loc);
    return moon ? s.moonH.alt - moonHorizon(s) : s.sunH.alt - SUN_HORIZON;
  };
  const ev = crossings(f, jd0, jd0 + 1);
  const rise = ev.find((e) => e.rising)?.jd;
  const set = ev.find((e) => !e.rising)?.jd;
  // Transit: the maximum altitude sampled every 5 minutes, refined to 1 minute.
  let tBest = jd0;
  let aBest = -Infinity;
  for (let jd = jd0; jd <= jd0 + 1; jd += 1 / 1440) {
    const s = snap(jd, loc);
    const h = moon ? s.moonH : s.sunH;
    if (Math.abs(h.ha) < 1 && h.alt > aBest) {
      aBest = h.alt;
      tBest = jd;
    }
  }
  const at = (jd: number | undefined) => (jd === undefined ? null : moon ? snap(jd, loc).moonH : snap(jd, loc).sunH);
  const up = f(jd0 + 0.5) > 0;
  return {
    rise: rise === undefined ? null : jdToUt(rise),
    set: set === undefined ? null : jdToUt(set),
    transit: aBest > -Infinity ? jdToUt(tBest) : null,
    riseAzDeg: at(rise)?.az ?? null,
    setAzDeg: at(set)?.az ?? null,
    transitAltDeg: aBest > -Infinity ? aBest : null,
    circumpolar: ev.length === 0 ? (up ? 'always-up' : 'always-down') : null,
  };
}

function trajectoryDay(p: TrajectoryParams): TrajectoryDay {
  const jd0 = utToJd({ ...p.date, hour: 0, minute: 0, second: 0 }) - p.tzOffsetHours / 24;
  const loc = p.location;
  const samples: TrackSample[] = Array.from({ length: 289 }, (_, i) => {
    const s = snap(jd0 + (i * 5) / 1440, loc);
    return {
      sunAltDeg: s.sunH.alt,
      sunAzDeg: s.sunH.az,
      moonAltDeg: s.moonH.alt,
      moonAzDeg: s.moonH.az,
      illuminatedFraction: s.k,
      elongationLonDeg: s.elongLon,
      limbZenithDeg: norm360(s.chi - s.q),
      moonDistanceKm: s.moon.distKm,
      moonGeoDistanceKm: s.moonGeo.distKm,
      moonSemiDiameterArcmin: moonSdArcmin(s.moon.distKm),
    };
  });
  const tw = (depression: number) => {
    const ev = crossings((jd) => snap(jd, loc).sunAirless + depression, jd0, jd0 + 1);
    const dawn = ev.find((e) => e.rising)?.jd;
    const dusk = ev.find((e) => !e.rising)?.jd;
    return { dawn: dawn === undefined ? null : jdToUt(dawn), dusk: dusk === undefined ? null : jdToUt(dusk) };
  };
  return {
    location: loc,
    date: p.date,
    tzOffsetHours: p.tzOffsetHours,
    timeZone: p.timeZone ?? null,
    ephemeris: 'mock (low precision)',
    dayStart: jdToUt(jd0),
    dayEnd: jdToUt(jd0 + 1),
    stepMinutes: 5,
    samples,
    sun: bodyEvents(jd0, loc, false),
    moon: bodyEvents(jd0, loc, true),
    civil: tw(6),
    nautical: tw(12),
    astronomical: tw(18),
    phases: phasesBetween(jd0 - 32, jd0 + 62),
  };
}

// --- Moon: optical libration and position angle (Meeus ch. 53, mean elements) ---

const INCL = 1.54242;

function libration(jd: number, lon: number, lat: number): { l: number; b: number } {
  const T = (jd - 2_451_545) / 36_525;
  const om = 125.0445479 - 1934.1362891 * T;
  const F = 93.272095 + 483_202.0175233 * T;
  const W = lon - om;
  const A = atan2d(sin(W) * cos(lat) * cos(INCL) - sin(lat) * sin(INCL), cos(W) * cos(lat));
  return { l: norm180(A - F), b: asind(-sin(W) * cos(lat) * sin(INCL) - sin(lat) * cos(INCL)) };
}

function physical(jd: number, moonEq: Eq, moon: Body, sun: Body): MoonPhysicalDto {
  const T = (jd - 2_451_545) / 36_525;
  const eps = obliquity(jd);
  const om = 125.0445479 - 1934.1362891 * T;
  const { l, b } = libration(jd, moon.lon, moon.lat);
  const X = sin(INCL) * sin(om);
  const Y = sin(INCL) * cos(om) * cos(eps) - cos(INCL) * sin(eps);
  const w = atan2d(X, Y);
  const P = norm360(asind((Math.hypot(X, Y) * cos(moonEq.ra - w)) / cos(b)));
  const ratio = moon.distKm / sun.distKm;
  const s = libration(jd, sun.lon + 180 + (ratio / D) * cos(moon.lat) * sin(sun.lon - moon.lon), ratio * moon.lat);
  return {
    libration: { opticalLonDeg: l, opticalLatDeg: b, physicalLonDeg: 0, physicalLatDeg: 0, lonDeg: l, latDeg: b },
    axisPositionAngleDeg: P,
    sunSelenographicLonDeg: s.l,
    sunSelenographicLatDeg: s.b,
    colongitudeDeg: norm360(90 - s.l),
  };
}

function moonView(p: MoonViewParams): MoonView {
  const jd = utToJd(p.instant);
  const loc = p.location ?? { latDeg: 0, lonDeg: 0, elevationM: 0 };
  const s = snap(jd, loc);
  const eps = obliquity(jd);
  const geoEq = toEq(s.moonGeo, eps);
  const geo = physical(jd, geoEq, s.moonGeo, s.sunGeo);
  const phases = phasesBetween(jd - 32, jd + 32);
  const after = (code: PhaseCode) => phases.find((x) => x.phase === code && utToJd(x.time) > jd)!.time;
  const lastNew = phases.filter((x) => x.phase === 'new' && utToJd(x.time) <= jd).pop()!.time;
  let track: LibrationTrack | null = null;
  if (p.includeTrack !== false) {
    const n0 = utToJd(lastNew);
    const n1 = utToJd(after('new'));
    const lb = (t: number): [number, number] => {
      const m = moonEcl(t);
      const x = libration(t, m.lon, m.lat);
      return [x.l, x.b];
    };
    const n = Math.ceil((n1 - n0) * 4);
    const full = phases.find((x) => x.phase === 'full' && utToJd(x.time) > n0)!.time;
    track = {
      newMoon: lastNew,
      fullMoon: full,
      nextNewMoon: after('new'),
      points: Array.from({ length: n + 1 }, (_, i) => lb(n0 + ((n1 - n0) * i) / n)),
      now: lb(jd),
      full: lb(utToJd(full)),
    };
  }
  const topo = !!p.location;
  const q = topo ? s.q : null;
  return {
    instant: p.instant,
    ephemeris: 'mock (low precision)',
    topocentric: topo,
    geocentric: geo,
    topocentricPhysical: topo ? geo : null,
    disc: {
      lDeg: geo.libration.lonDeg,
      bDeg: geo.libration.latDeg,
      sunLonDeg: geo.sunSelenographicLonDeg,
      sunLatDeg: geo.sunSelenographicLatDeg,
      northUpRotationDeg: geo.axisPositionAngleDeg,
      zenithUpRotationDeg: q === null ? null : norm360(geo.axisPositionAngleDeg - q),
    },
    illuminatedFraction: s.k,
    phaseAngleDeg: atan2d(s.sun.distKm * sin(s.elong), s.moon.distKm - s.sun.distKm * cos(s.elong)),
    elongationDeg: s.elong,
    brightLimbDeg: s.chi,
    parallacticDeg: q,
    brightLimbZenithDeg: q === null ? null : norm360(s.chi - q),
    elongationLonDeg: s.elongLon,
    phaseName: '(mock)',
    ageDays: jd - utToJd(lastNew),
    lastNewMoon: lastNew,
    nextNewMoon: after('new'),
    nextFirstQuarter: after('first-quarter'),
    nextFullMoon: after('full'),
    nextLastQuarter: after('last-quarter'),
    distanceKm: topo ? s.moon.distKm : s.moonGeo.distKm,
    geocentricDistanceKm: s.moonGeo.distKm,
    apparentDiameterArcmin: 2 * moonSdArcmin(topo ? s.moon.distKm : s.moonGeo.distKm),
    raDeg: s.moon.ra,
    decDeg: s.moon.dec,
    altDeg: topo ? s.moonH.alt : null,
    azDeg: topo ? s.moonH.az : null,
    crater: null,
    librationTrack: track,
  };
}

const FEATURES: MoonFeatures = {
  craters: [
    ['Tycho', -43.3, -11.4, 85],
    ['Copernicus', 9.6, -20.1, 96],
    ['Kepler', 8.1, -38.0, 31],
    ['Aristarchus', 23.7, -47.5, 40],
    ['Plato', 51.6, -9.4, 101],
    ['Clavius', -58.6, -14.7, 231],
    ['Langrenus', -8.9, 61.0, 132],
    ['Petavius', -25.3, 60.4, 184],
    ['Theophilus', -11.4, 26.3, 99],
    ['Grimaldi', -5.5, -68.3, 173],
  ].map(([name, latDeg, lonDeg, diameterKm]) => ({
    name: name as string,
    latDeg: latDeg as number,
    lonDeg: lonDeg as number,
    diameterKm: diameterKm as number,
  })),
  maria: (
    [
      ['Mare Imbrium', 32.8, -15.6, 1146],
      ['Mare Serenitatis', 28.0, 17.5, 707],
      ['Mare Crisium', 17.0, 59.1, 556],
      ['Mare Nectaris', -15.2, 35.5, 333],
      ['Mare Humorum', -24.4, -38.6, 389],
      ['Mare Tranquillitatis', 8.5, 31.4, 700],
      ['Mare Fecunditatis', -7.8, 51.3, 600],
      ['Oceanus Procellarum', 18.4, -57.4, 1500],
    ] as [string, number, number, number][]
  ).map(([name, lat, lon, dKm]) => {
    // A small circle of the given diameter (mock: no polygons).
    const r = dKm / 2 / 1737.4 / D;
    const outline: number[] = [];
    for (let i = 0; i <= 48; i++) {
      const az = (360 * i) / 48;
      const lat2 = asind(sin(lat) * cos(r) + cos(lat) * sin(r) * cos(az));
      const lon2 = lon + atan2d(sin(az) * sin(r) * cos(lat), cos(r) - sin(lat) * sin(lat2));
      outline.push(lat2, lon2);
    }
    return { name, latDeg: lat, lonDeg: lon, circular: true, outline };
  }),
  moonRadiusKm: 1737.4,
  attribution: 'Mock subset of the IAU Gazetteer of Planetary Nomenclature (public domain).',
};

const later = <T>(f: () => T, ms = 20): Promise<T> =>
  new Promise((resolve, reject) =>
    setTimeout(() => {
      try {
        resolve(f());
      } catch (err) {
        reject(BackendError.from(err));
      }
    }, ms),
  );

export function mockSkyApi(): SkyApi {
  return {
    skyScene: (p) => later(() => skyScene(p)),
    skyStars: (p) => later(() => skyStars(p)),
    trajectoryDay: (p) => later(() => trajectoryDay(p)),
    moonView: (p) => later(() => moonView(p)),
    moonFeatures: () => later(() => structuredClone(FEATURES)),
  };
}
