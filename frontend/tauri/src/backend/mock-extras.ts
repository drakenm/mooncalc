// In-browser stand-ins for the calendar and ephemeris-table services, for
// `npm run dev` outside Tauri. Every calendar is the arithmetical Tabular II
// calendar (the astronomical-epoch ones a day earlier) and the tables are
// mean-motion approximations or canned 2024 values: right shapes, rough
// numbers. Real results need the desktop app.

import type { CivilDate, Location, UtDateTime } from './types';
import type {
  CalendarApi,
  CalendarCatalog,
  CalendarDay,
  CalendarDescription,
  CalendarSpec,
  HijriYmd,
  MonthRef,
  MonthStart,
} from './types-calendar';
import type { EclipseInfo, EphemerisRow, RiseSetDay, TablesApi } from './types-tables';

// --- civil calendar (Julian before 1582-10-15, Gregorian after) -------------

const REFORM_JDN = 2_299_161;

export function civilToJdn(d: CivilDate): number {
  const a = Math.floor((14 - d.month) / 12);
  const y = d.year + 4800 - a;
  const m = d.month + 12 * a - 3;
  const greg = d.day + Math.floor((153 * m + 2) / 5) + 365 * y + Math.floor(y / 4) - Math.floor(y / 100) + Math.floor(y / 400) - 32045;
  if (greg >= REFORM_JDN) return greg;
  return d.day + Math.floor((153 * m + 2) / 5) + 365 * y + Math.floor(y / 4) - 32083;
}

export function jdnToCivil(jdn: number): CivilDate {
  let c: number;
  let b: number;
  if (jdn >= REFORM_JDN) {
    const a = jdn + 32044;
    b = Math.floor((4 * a + 3) / 146097);
    c = a - Math.floor((146097 * b) / 4);
  } else {
    b = 0;
    c = jdn + 32082;
  }
  const d = Math.floor((4 * c + 3) / 1461);
  const e = c - Math.floor((1461 * d) / 4);
  const m = Math.floor((5 * e + 2) / 153);
  return {
    day: e - Math.floor((153 * m + 2) / 5) + 1,
    month: m + 3 - 12 * Math.floor(m / 10),
    year: 100 * b + d - 4800 + Math.floor(m / 10),
  };
}

// --- tabular Hijri calendar (type II, civil epoch) -------------------------

const EPOCH = 1_948_440;

function hijriToJdn(h: HijriYmd, offset: number): number {
  return h.day + Math.ceil(29.5 * (h.month - 1)) + (h.year - 1) * 354 + Math.floor((3 + 11 * h.year) / 30) + EPOCH - 1 + offset;
}

function jdnToHijri(jdn: number, offset: number): HijriYmd {
  const j = jdn - offset;
  const year = Math.floor((30 * (j - EPOCH) + 10646) / 10631);
  const m = Math.min(12, Math.ceil((j - (29 + hijriToJdn({ year, month: 1, day: 1 }, 0))) / 29.5) + 1);
  const month = Math.max(1, m);
  return { year, month, day: j - hijriToJdn({ year, month, day: 1 }, 0) + 1 };
}

function monthLength(year: number, month: number, offset: number): number {
  const next = month === 12 ? { year: year + 1, month: 1, day: 1 } : { year, month: month + 1, day: 1 };
  return hijriToJdn(next, offset) - hijriToJdn({ year, month, day: 1 }, offset);
}

const offsetOf = (id: string) => (id.endsWith('astronomical') ? -1 : 0);

const MONTHS: [string, string, string][] = [
  ['Muḥarram', 'Muharram', 'محرم'],
  ['Ṣafar', 'Safar', 'صفر'],
  ['Rabīʿ al-Awwal', 'Rabi al-Awwal', 'ربيع الأول'],
  ['Rabīʿ al-Thānī', 'Rabi al-Thani', 'ربيع الآخر'],
  ['Jumādā al-Ūlā', 'Jumada al-Ula', 'جمادى الأولى'],
  ['Jumādā al-Ākhirah', 'Jumada al-Akhirah', 'جمادى الآخرة'],
  ['Rajab', 'Rajab', 'رجب'],
  ['Shaʿbān', 'Shaban', 'شعبان'],
  ['Ramaḍān', 'Ramadan', 'رمضان'],
  ['Shawwāl', 'Shawwal', 'شوال'],
  ['Dhū al-Qaʿdah', 'Dhu al-Qadah', 'ذو القعدة'],
  ['Dhū al-Ḥijjah', 'Dhu al-Hijjah', 'ذو الحجة'],
];

const WEEKDAYS: [string, string, string][] = [
  ['Sunday', 'al-Aḥad', 'الأحد'],
  ['Monday', 'al-Ithnayn', 'الاثنين'],
  ['Tuesday', 'al-Thulāthāʾ', 'الثلاثاء'],
  ['Wednesday', 'al-Arbiʿāʾ', 'الأربعاء'],
  ['Thursday', 'al-Khamīs', 'الخميس'],
  ['Friday', 'al-Jumuʿah', 'الجمعة'],
  ['Saturday', 'al-Sabt', 'السبت'],
];

function desc(id: string, name: string, shortLabel: string, family: CalendarDescription['family'], computed: boolean): CalendarDescription {
  return {
    id,
    name,
    shortLabel,
    description: `${name} (mock: tabular arithmetic in the browser).`,
    family,
    computed,
    cldrId: null,
    attribution: null,
    reference: null,
    firstYear: computed ? null : 1,
    lastYear: computed ? null : 9999,
  };
}

const CATALOG: CalendarCatalog = {
  calendars: [
    desc('umm-al-qura', 'Umm al-Qura (Saudi Arabia)', 'Umm al-Qura', 'umm-al-qura', false),
    desc('umm-al-qura-icu', 'Umm al-Qura (ICU/CLDR table)', 'UQ (ICU)', 'umm-al-qura', false),
    desc('visibility', 'Crescent visibility (computed)', 'Visibility', 'visibility', true),
    desc('rule-umm-al-qura', 'Umm al-Qura rule (computed)', 'UQ rule', 'rule', true),
    desc('rule-conjunction-before-sunset-mecca', 'Conjunction before sunset at Mecca', 'Conj<sunset', 'rule', true),
    desc('rule-conjunction-before-noon-ut', 'Conjunction before 12:00 UT (FCNA/ISNA)', 'Conj<12 UT', 'rule', true),
    desc('tabular-2-civil', 'Tabular II (civil epoch)', 'II civil', 'tabular', false),
    desc('tabular-2-astronomical', 'Tabular II (astronomical epoch)', 'II astr.', 'tabular', false),
  ],
  defaultId: 'umm-al-qura',
  regions: [
    { id: 'world', name: 'Anywhere (60° S – 60° N)' },
    { id: 'americas', name: 'The Americas' },
    { id: 'europe-africa-middle-east', name: 'Europe, Africa and the Middle East' },
    { id: 'asia-pacific', name: 'Asia and the Pacific' },
  ],
  hijriMonths: MONTHS.map(([name, ascii, arabic], i) => ({ number: i + 1, name, ascii, arabic })),
  weekdays: WEEKDAYS.map(([english, transliterated, arabic], index) => ({ index, english, transliterated, arabic })),
  dayConvention:
    'The Islamic day begins at sunset. As in almanacs and official calendars, each Hijri date is shown on the civil day (midnight to midnight) that begins after its first sunset: the evening before already belongs to it.',
};

const weekdayOf = (jdn: number) => (((jdn + 1) % 7) + 7) % 7;

function day(jdn: number, offset: number): CalendarDay {
  return { jdn, civil: jdnToCivil(jdn), hijri: jdnToHijri(jdn, offset), weekday: weekdayOf(jdn) };
}

const step = (m: MonthRef, delta: number): MonthRef => {
  const i = m.year * 12 + m.month - 1 + delta;
  return { year: Math.floor(i / 12), month: (((i % 12) + 12) % 12) + 1 };
};

function nameOf(spec: CalendarSpec): string {
  return CATALOG.calendars.find((c) => c.id === spec.id)?.name ?? spec.id;
}

function start(year: number, month: number, offset: number): MonthStart {
  const jdn = hijriToJdn({ year, month, day: 1 }, offset);
  return { year, month, jdn, civil: jdnToCivil(jdn) };
}

export const mockCalendarApi: CalendarApi = {
  list: async () => CATALOG,
  monthStarts: async (p) => {
    const o = offsetOf(p.calendar.id);
    const starts = Array.from({ length: 13 }, (_, i) => start(p.hijriYear + Math.floor(i / 12), (i % 12) + 1, o));
    return { calendarId: p.calendar.id, calendarName: nameOf(p.calendar), hijriYear: p.hijriYear, starts };
  },
  month: async (p) => {
    const o = offsetOf(p.calendar.id);
    const primary = p.primary ?? 'hijri';
    let month: MonthRef;
    if (p.month) month = p.month;
    else if (p.date) {
      if (primary === 'gregorian') month = { year: p.date.year, month: p.date.month };
      else {
        const h = jdnToHijri(civilToJdn(p.date), o);
        month = { year: h.year, month: h.month };
      }
    } else throw { code: 'invalid-params', message: 'calendar_month needs a month or a date' };
    let first: number;
    let len: number;
    if (primary === 'hijri') {
      first = hijriToJdn({ year: month.year, month: month.month, day: 1 }, o);
      len = monthLength(month.year, month.month, o);
    } else {
      first = civilToJdn({ year: month.year, month: month.month, day: 1 });
      const n = step(month, 1);
      len = civilToJdn({ year: n.year, month: n.month, day: 1 }) - first;
    }
    const ws = p.weekStart ?? 6;
    const lead = (weekdayOf(first) - ws + 7) % 7;
    const cells: (CalendarDay | null)[] = [...Array<null>(lead).fill(null), ...Array.from({ length: len }, (_, i) => day(first + i, o))];
    while (cells.length % 7) cells.push(null);
    const weeks = Array.from({ length: cells.length / 7 }, (_, i) => cells.slice(i * 7, i * 7 + 7));
    const other: MonthRef[] = [];
    for (let j = first; j < first + len; j++) {
      const d = primary === 'hijri' ? jdnToCivil(j) : jdnToHijri(j, o);
      const last = other[other.length - 1];
      if (!last || last.year !== d.year || last.month !== d.month) other.push({ year: d.year, month: d.month });
    }
    return {
      calendarId: p.calendar.id,
      calendarName: nameOf(p.calendar),
      primary,
      month,
      prev: step(month, -1),
      next: step(month, 1),
      columns: Array.from({ length: 7 }, (_, i) => (ws + i) % 7),
      weeks,
      otherMonths: other,
      message: null,
      missing: [],
    };
  },
  convert: async (p) => {
    const o = offsetOf(p.calendar.id);
    if (p.civil) {
      const jdn = civilToJdn(p.civil);
      const d = day(jdn, o);
      return { day: d, monthLength: d.hijri ? monthLength(d.hijri.year, d.hijri.month, o) : null, message: null, missing: [] };
    }
    if (p.hijri) {
      const len = monthLength(p.hijri.year, p.hijri.month, o);
      if (p.hijri.day > len) return { day: null, monthLength: len, message: `No such date: this month has ${len} days.`, missing: [] };
      return { day: day(hijriToJdn(p.hijri, o), o), monthLength: len, message: null, missing: [] };
    }
    throw { code: 'invalid-params', message: 'give exactly one of civil and hijri' };
  },
  compare: async (p) => {
    const cols = CATALOG.calendars.filter((c) => !(p.builtinOnly && c.computed) && (c.id !== 'visibility' || p.visibility));
    const rows = Array.from({ length: 12 }, (_, i) => {
      const starts = cols.map((c) => start(p.hijriYear, i + 1, offsetOf(c.id)));
      const consensus = starts[0]?.jdn ?? null;
      const differs = starts.map((s) => s.jdn !== consensus);
      return { month: i + 1, starts, consensus, disagrees: differs.some(Boolean), differs };
    });
    return {
      hijriYear: p.hijriYear,
      columns: cols.map((c) => ({ id: c.id, label: c.shortLabel, name: c.name, description: c.description, computed: c.computed })),
      rows,
      missing: [],
    };
  },
};

// --- ephemeris tables -------------------------------------------------------

const SYNODIC = 29.530588861;
const DEG = Math.PI / 180;

function jdOf(t: UtDateTime): number {
  return civilToJdn(t) - 0.5 + (t.hour + t.minute / 60 + t.second / 3600) / 24;
}

function utOf(jd: number): UtDateTime {
  const jdn = Math.floor(jd + 0.5);
  const c = jdnToCivil(jdn);
  let minutes = Math.round((jd + 0.5 - jdn) * 1440);
  if (minutes >= 1440) return utOf(jdn + 0.5);
  const hour = Math.floor(minutes / 60);
  minutes -= hour * 60;
  return { ...c, hour, minute: minutes, second: 0 };
}

const norm360 = (x: number) => ((x % 360) + 360) % 360;

function mockRow(jd: number, loc: Location | null): EphemerisRow {
  const d = jd - 2451545;
  const sunLon = norm360(280.46 + 0.9856474 * d);
  const moonLon = norm360(218.32 + 13.176396 * d);
  const elong = norm360(moonLon - sunLon);
  const age = (elong / 360) * SYNODIC;
  const eps = 23.44 * DEG;
  const eq = (lon: number, lat = 0) => {
    const l = lon * DEG;
    const b = lat * DEG;
    const ra = Math.atan2(Math.sin(l) * Math.cos(eps) - Math.tan(b) * Math.sin(eps), Math.cos(l)) / DEG;
    const dec = Math.asin(Math.sin(b) * Math.cos(eps) + Math.cos(b) * Math.sin(eps) * Math.sin(l)) / DEG;
    return { ra: norm360(ra), dec };
  };
  const moonLat = 5.1 * Math.sin((93.27 + 13.22935 * d) * DEG);
  const s = eq(sunLon);
  const m = eq(moonLon, moonLat);
  const topo = (ra: number, dec: number, dist: number, sd: number) => {
    if (!loc) return null;
    const lst = norm360(280.46061837 + 360.98564736629 * d + loc.lonDeg);
    const ha = (lst - ra) * DEG;
    const lat = loc.latDeg * DEG;
    const alt = Math.asin(Math.sin(lat) * Math.sin(dec * DEG) + Math.cos(lat) * Math.cos(dec * DEG) * Math.cos(ha)) / DEG;
    const az = norm360(Math.atan2(-Math.sin(ha), Math.tan(dec * DEG) * Math.cos(lat) - Math.sin(lat) * Math.cos(ha)) / DEG);
    return { altDeg: alt, altAirlessDeg: alt, azDeg: az, raDeg: ra, decDeg: dec, distanceKm: dist, semidiameterArcmin: sd };
  };
  const conj = jd - age;
  return {
    time: utOf(jd),
    jdUt: jd,
    jdTt: jd + 69 / 86400,
    sun: { raDeg: s.ra, decDeg: s.dec, lonDeg: sunLon, latDeg: 0, distanceKm: 149_597_870, semidiameterArcmin: 16, parallaxArcmin: 0.146, topocentric: topo(s.ra, s.dec, 149_597_870, 16) },
    moon: { raDeg: m.ra, decDeg: m.dec, lonDeg: moonLon, latDeg: moonLat, distanceKm: 384_400, semidiameterArcmin: 15.5, parallaxArcmin: 57, topocentric: topo(m.ra, m.dec, 384_400, 15.7) },
    elongationDeg: elong > 180 ? 360 - elong : elong,
    phaseAngleDeg: 180 - (elong > 180 ? 360 - elong : elong),
    illuminatedFraction: (1 - Math.cos(elong * DEG)) / 2,
    brightLimbDeg: elong < 180 ? 250 : 70,
    topocentricElongationDeg: loc ? (elong > 180 ? 360 - elong : elong) : null,
    moonAgeDays: age,
    conjunction: utOf(conj),
    libration: { lonDeg: 6 * Math.sin(d / 27.55 * 2 * Math.PI), latDeg: 6.5 * Math.sin(d / 27.21 * 2 * Math.PI), axisPositionAngleDeg: 20 * Math.sin(d / 27.21 * 2 * Math.PI), colongitudeDeg: norm360(270 - elong), sunLatDeg: 1.5 * Math.sin(d / 346.6 * 2 * Math.PI) },
  };
}

/** UT for a local clock time on a civil date. */
function localUt(date: CivilDate, hours: number, tz: number): UtDateTime {
  return utOf(civilToJdn(date) - 0.5 + (hours - tz) / 24);
}

const CANNED_ECLIPSES: EclipseInfo[] = [
  { body: 'lunar', kind: 'penumbral', label: 'Lunar penumbral', lunation: 290, greatest: { year: 2024, month: 3, day: 25, hour: 7, minute: 12, second: 0 }, gamma: 1.0607, magnitude: -0.1322, solar: null, lunar: { penumbralMagnitude: 0.9577, umbralMagnitude: -0.1322, contacts: [], penumbralDurationMin: 280, partialDurationMin: null, totalDurationMin: null, zenithLatDeg: 1.8, zenithLonDeg: -105, visible: null } },
  { body: 'solar', kind: 'total', label: 'Solar total', lunation: 1253, greatest: { year: 2024, month: 4, day: 8, hour: 18, minute: 17, second: 0 }, gamma: 0.3431, magnitude: 1.0566, solar: { central: true, greatestLatDeg: 25.3, greatestLonDeg: -104.1, sunAltDeg: 69.8, local: null }, lunar: null },
  { body: 'lunar', kind: 'partial', label: 'Lunar partial', lunation: 1258, greatest: { year: 2024, month: 9, day: 18, hour: 2, minute: 44, second: 0 }, gamma: -0.9792, magnitude: 0.0848, solar: null, lunar: { penumbralMagnitude: 1.0367, umbralMagnitude: 0.0848, contacts: [], penumbralDurationMin: 243, partialDurationMin: 63, totalDurationMin: null, zenithLatDeg: -2.2, zenithLonDeg: -41.4, visible: null } },
  { body: 'solar', kind: 'annular', label: 'Solar annular', lunation: 1259, greatest: { year: 2024, month: 10, day: 2, hour: 18, minute: 45, second: 0 }, gamma: -0.3509, magnitude: 0.9326, solar: { central: true, greatestLatDeg: -22.0, greatestLonDeg: -114.5, sunAltDeg: 69.4, local: null }, lunar: null },
];

export const mockTablesApi: TablesApi = {
  ephemerisTable: async (p) => {
    const jd0 = jdOf(p.start);
    const rows = Array.from({ length: Math.min(p.count, 10_000) }, (_, i) => mockRow(jd0 + (i * p.stepMinutes) / 1440, p.location ?? null));
    return { rows, ephemeris: 'Mock (mean motions)' };
  },
  riseSetMonth: async (p) => {
    const days: RiseSetDay[] = [];
    for (let d = 1; d <= 31; d++) {
      const date = { year: p.year, month: p.month, day: d };
      if (jdnToCivil(civilToJdn(date)).month !== p.month) break;
      const shift = (d * 50) / 60;
      const ev = (h: number, angleDeg: number) => ({ time: localUt(date, h, p.tzOffsetHours), angleDeg });
      days.push({
        date,
        sun: { rise: ev(6.2, 95), transit: ev(12.2, 60), set: ev(18.2, 265), circumpolar: null },
        moon: { rise: ev((7 + shift) % 24, 100), transit: ev((13 + shift) % 24, 55), set: ev((19 + shift) % 24, 260), circumpolar: null },
        civil: { dawn: localUt(date, 5.8, p.tzOffsetHours), dusk: localUt(date, 18.6, p.tzOffsetHours) },
        nautical: { dawn: localUt(date, 5.3, p.tzOffsetHours), dusk: localUt(date, 19.1, p.tzOffsetHours) },
        astronomical: { dawn: localUt(date, 4.9, p.tzOffsetHours), dusk: localUt(date, 19.5, p.tzOffsetHours) },
      });
    }
    return { year: p.year, month: p.month, tzOffsetHours: p.tzOffsetHours, location: p.location, days, ephemeris: 'Mock' };
  },
  lunarPhases: async (p) => {
    const jd0 = civilToJdn({ year: p.year, month: 1, day: 1 }) - 0.5;
    const jd1 = civilToJdn({ year: p.year + 1, month: 1, day: 1 }) - 0.5;
    const k0 = Math.floor((jd0 - 2451550.1) / SYNODIC) - 1;
    const lunations = [];
    for (let k = k0; ; k++) {
      const nm = 2451550.1 + k * SYNODIC;
      if (nm >= jd1) break;
      const phases = [0, 0.25, 0.5, 0.75].map((f) => {
        const jd = nm + f * SYNODIC;
        return jd >= jd0 && jd < jd1 ? utOf(jd) : null;
      }) as [UtDateTime | null, UtDateTime | null, UtDateTime | null, UtDateTime | null];
      if (phases.every((x) => x === null)) continue;
      const ages = [0, 0.25, 0.5, 0.75].map((f, i) => (phases[i] ? f * SYNODIC : null)) as [number | null, number | null, number | null, number | null];
      lunations.push({ k, newMoon: utOf(nm), lengthDays: SYNODIC, phases, agesDays: ages });
    }
    return { year: p.year, lunations, ephemeris: 'Mock (mean phases)' };
  },
  apsidesNodes: async (p) => {
    const jd0 = civilToJdn({ year: p.year, month: 1, day: 1 }) - 0.5;
    const n = (period: number, phase: number) => Array.from({ length: Math.floor(365 / period) + 1 }, (_, i) => jd0 + phase + i * period).filter((j) => j < jd0 + 365);
    return {
      year: p.year,
      apsides: [...n(27.5546, 9).map((j) => ({ kind: 'perigee' as const, time: utOf(j), distanceKm: 362_000, parallaxArcsec: 3630, diameterArcmin: 33 })), ...n(27.5546, 23).map((j) => ({ kind: 'apogee' as const, time: utOf(j), distanceKm: 405_000, parallaxArcsec: 3247, diameterArcmin: 29.5 }))].sort((a, b) => jdOf(a.time) - jdOf(b.time)),
      nodes: n(13.6061, 5).map((j, i) => ({ kind: i % 2 ? ('descending' as const) : ('ascending' as const), time: utOf(j) })),
      declinations: n(13.6608, 12).map((j, i) => ({ kind: i % 2 ? ('south' as const) : ('north' as const), time: utOf(j), decDeg: i % 2 ? -28.4 : 28.4 })),
      ephemeris: 'Mock (mean periods)',
    };
  },
  eclipses: async (p) =>
    ({
      firstYear: p.firstYear,
      lastYear: p.lastYear,
      location: p.location ?? null,
      eclipses: CANNED_ECLIPSES.filter((e) => e.greatest.year >= p.firstYear && e.greatest.year <= p.lastYear && (p.filter === undefined || p.filter === 'any' || p.filter === e.body)),
      ephemeris: 'Mock (canned 2024 eclipses, no local circumstances)',
    }),
  seasons: async (p) => {
    const at = (m: number, d: number, h: number) => ({ year: p.year, month: m, day: d, hour: h, minute: 0, second: 0 });
    const eot = (n: number) => {
      const b = ((360 / 365) * (n - 81)) * DEG;
      return 9.87 * Math.sin(2 * b) - 7.53 * Math.cos(b) - 1.5 * Math.sin(b);
    };
    const jan1 = civilToJdn({ year: p.year, month: 1, day: 1 });
    const len = civilToJdn({ year: p.year + 1, month: 1, day: 1 }) - jan1;
    const dayOf = (m: number, d: number) => civilToJdn({ year: p.year, month: m, day: d }) - jan1 + 1;
    return {
      year: p.year,
      seasons: [
        { kind: 'march-equinox', name: 'March equinox', time: at(3, 20, 3), equationOfTimeMin: eot(dayOf(3, 20)) },
        { kind: 'june-solstice', name: 'June solstice', time: at(6, 20, 21), equationOfTimeMin: eot(dayOf(6, 20)) },
        { kind: 'september-equinox', name: 'September equinox', time: at(9, 22, 13), equationOfTimeMin: eot(dayOf(9, 22)) },
        { kind: 'december-solstice', name: 'December solstice', time: at(12, 21, 9), equationOfTimeMin: eot(dayOf(12, 21)) },
      ],
      equationOfTime: Array.from({ length: len }, (_, i) => ({ date: jdnToCivil(jan1 + i), minutes: eot(i + 1) })),
      ephemeris: 'Mock (approximate)',
    };
  },
};
