// In-browser stand-in backend for `npm run dev` outside Tauri.
//
// Produces the wire shapes of crates/mooncalc-app with placeholder content so
// the UI can be developed in any browser: a synthetic zone pattern (no
// astronomy), empty report values, a handful of cities and no base map. The
// real results need the desktop app (or, later, the WASM backend).

import { BackendError, type Backend } from './backend';
import { bytesToBase64 } from './codec';
import { mockSkyApi } from './mock-sky';
import { mockCalendarApi, mockTablesApi } from './mock-extras';
import {
  NO_DATA,
  type AppInfo,
  type Basemap,
  type BasemapParams,
  type City,
  type CitySearchParams,
  type CrescentReport,
  type CriterionInfo,
  type EarliestParams,
  type EarliestResult,
  type EphemerisBackendInfo,
  type ExportPngParams,
  type LoadDe440Params,
  type LocationReport,
  type LocationReportParams,
  type MapParams,
  type MapResult,
  type NearestCity,
  type NearestCityParams,
  type PngImage,
  type ZoneLegendEntry,
} from './types';

const zone = (code: string, description: string, rgb: [number, number, number]): ZoneLegendEntry => ({
  code,
  description,
  rgb,
});

const CRITERIA: CriterionInfo[] = [
  {
    id: 'yallop',
    name: 'Yallop (1997)',
    reference: 'Yallop, B. D. (1997), NAO Technical Note No. 69.',
    evaluationTime: 'Best time (sunset + 4/9 lag)',
    thresholds: [0.216, -0.014, -0.16, -0.232, -0.293],
    zones: [
      zone('A', 'Easily visible (ARCL ≥ 12°)', [46, 158, 63]),
      zone('B', 'Visible under perfect conditions', [166, 216, 84]),
      zone('C', 'May need optical aid to find the crescent', [255, 217, 47]),
      zone('D', 'Will need optical aid to find the crescent', [252, 141, 89]),
      zone('E', 'Not visible with a telescope (ARCL ≤ 8.5°)', [215, 48, 39]),
      zone('F', 'Not visible, below the Danjon limit (ARCL ≤ 8°)', [150, 150, 150]),
    ],
  },
  {
    id: 'odeh',
    name: 'Odeh (2004)',
    reference: 'Odeh, M. Sh. (2004), Experimental Astronomy 18, 39-64.',
    evaluationTime: 'Best time (sunset + 4/9 lag)',
    thresholds: [5.65, 2, -0.96],
    zones: [
      zone('A', 'Visible by naked eye', [46, 160, 67]),
      zone('B', 'Visible by optical aid, could be seen by naked eye', [154, 209, 75]),
      zone('C', 'Visible by optical aid only', [240, 140, 46]),
      zone('D', 'Not visible even with optical aid', [214, 60, 60]),
    ],
  },
];

const CITIES: City[] = [
  ['Makkah', 'Saudi Arabia', 'SA', 21.4225, 39.8262, 277, 'Asia/Riyadh', 3],
  ['Cairo', 'Egypt', 'EG', 30.0444, 31.2357, 23, 'Africa/Cairo', 2],
  ['Istanbul', 'Turkey', 'TR', 41.0082, 28.9784, 39, 'Europe/Istanbul', 3],
  ['Karachi', 'Pakistan', 'PK', 24.8607, 67.0011, 10, 'Asia/Karachi', 5],
  ['Jakarta', 'Indonesia', 'ID', -6.2088, 106.8456, 8, 'Asia/Jakarta', 7],
  ['London', 'United Kingdom', 'GB', 51.5074, -0.1278, 11, 'Europe/London', 0],
  ['New York', 'United States', 'US', 40.7128, -74.006, 10, 'America/New_York', -5],
  ['Cape Town', 'South Africa', 'ZA', -33.9249, 18.4241, 15, 'Africa/Johannesburg', 2],
  ['Sydney', 'Australia', 'AU', -33.8688, 151.2093, 58, 'Australia/Sydney', 10],
].map(([name, country, countryCode, latDeg, lonDeg, elevationM, timeZone, offset]) => ({
  name: name as string,
  admin1: null,
  country: country as string,
  countryCode: countryCode as string,
  location: { latDeg: latDeg as number, lonDeg: lonDeg as number, elevationM: elevationM as number },
  timeZone: timeZone as string,
  stdOffsetHours: offset as number,
  dstOffsetHours: offset as number,
  population: null,
}));

const BACKENDS: EphemerisBackendInfo[] = [
  { id: 'meeus', name: 'Meeus / ELP-2000/82', description: 'Built-in analytical theory.', available: true, needsFile: false, ready: true, file: null, detail: null },
  { id: 'de440', name: 'JPL DE440', description: 'Desktop builds with the de440 feature only.', available: false, needsFile: true, ready: false, file: null, detail: null },
];

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const unsupported = (what: string) => new BackendError('unsupported', `${what} needs the desktop app (mock backend)`);

function criterion(id: string): CriterionInfo {
  const c = CRITERIA.find((x) => x.id === id);
  if (!c) throw new BackendError('unknown-criterion', `unknown criterion id "${id}"`);
  return c;
}

function emptyCrescent(crescent: 'evening' | 'morning'): CrescentReport {
  return {
    crescent,
    status: 'crescent',
    statusText: 'Mock backend: no computation in the browser preview.',
    sunEvent: null,
    moonEvent: null,
    bestTime: null,
    conjunction: null,
    geometry: null,
    criteria: CRITERIA.map((c) => ({
      criterionId: c.id,
      name: c.name,
      evaluationTime: c.evaluationTime,
      evaluatedAt: null,
      status: 'crescent',
      verdict: null,
    })),
  };
}

export class MockBackend implements Backend {
  readonly kind = 'mock' as const;
  readonly calendar = mockCalendarApi;
  readonly tables = mockTablesApi;

  async appInfo(): Promise<AppInfo> {
    return {
      name: 'MoonCalc',
      version: '0.1.0',
      runtime: 'mock (browser)',
      ephemerisBackends: structuredClone(BACKENDS),
      defaultEphemeris: 'meeus',
      attributions: [],
    };
  }

  async criteriaList(): Promise<CriterionInfo[]> {
    return structuredClone(CRITERIA);
  }

  async computeVisibilityMap(p: MapParams): Promise<MapResult> {
    const c = criterion(p.criterionId);
    if (!(p.resolutionDeg >= 0.1 && p.resolutionDeg <= 10)) {
      throw new BackendError('invalid-params', 'resolutionDeg must be between 0.1 and 10');
    }
    const start = performance.now();
    const rows = Math.max(1, Math.round(180 / p.resolutionDeg));
    const cols = Math.max(1, Math.round(360 / p.resolutionDeg));
    const cells = new Uint8Array(rows * cols).fill(NO_DATA);
    // An arbitrary smooth pattern that moves with the date; not astronomy.
    const n = p.date.year * 372 + p.date.month * 31 + p.date.day;
    const lon0 = (((n * 47) % 360) + 360) % 360 - 180;
    const lat0 = 15 * Math.sin(n * 0.21);
    const peakLon = p.crescent === 'evening' ? lon0 - 90 : lon0 + 90;
    const maxZone = c.zones.length - 1;
    const zoneCounts = new Array<number>(c.zones.length).fill(0);
    for (let r = 0; r < rows; r++) {
      const lat = 90 - ((r + 0.5) * 180) / rows;
      if (Math.abs(lat) > 62) continue;
      const latTerm = ((lat - lat0) / 40) ** 2;
      for (let col = 0; col < cols; col++) {
        const lon = -180 + ((col + 0.5) * 360) / cols;
        const s = Math.cos(((lon - peakLon) * Math.PI) / 180) - latTerm;
        if (s < -0.9) continue;
        const z = Math.min(maxZone, Math.max(0, Math.floor((0.8 - s) / 0.3)));
        cells[r * cols + col] = z;
        zoneCounts[z]!++;
      }
    }
    const totalMs = performance.now() - start;
    await delay(50);
    return {
      criterionId: c.id,
      criterionName: `${c.name} (synthetic)`,
      date: p.date,
      crescent: p.crescent,
      grid: { latMinDeg: -90, latMaxDeg: 90, lonMinDeg: -180, lonMaxDeg: 180, rows, cols },
      cells: bytesToBase64(cells),
      noData: NO_DATA,
      legend: structuredClone(c.zones),
      values: null,
      thresholds: c.thresholds,
      zoneCounts,
      statusCounts: { crescent: zoneCounts.reduce((a, b) => a + b, 0), beforeConjunction: 0, moonSetsFirst: 0, moonAlwaysUp: 0, moonAlwaysDown: 0, midnightSun: 0, polarNight: 0, sunAltitudeNotReached: 0 },
      conjunction: { ...p.date, hour: 0, minute: 0, second: 0 },
      evaluationTime: c.evaluationTime,
      best: null,
      ephemeris: 'mock',
      timing: { prepareMs: 0, evaluateMs: totalMs, encodeMs: 0, totalMs, threads: 1 },
    };
  }

  async exportMapPng(_p: ExportPngParams): Promise<PngImage> {
    throw unsupported('PNG export');
  }

  async locationReport(p: LocationReportParams): Promise<LocationReport> {
    await delay(30);
    return {
      location: p.location,
      date: p.date,
      tzOffsetHours: p.tzOffsetHours,
      ephemeris: 'mock',
      day: { sunrise: null, sunset: null, solarNoon: null, moonrise: null, moonset: null, moonTransit: null, civilDawn: null, civilDusk: null, nauticalDawn: null, nauticalDusk: null },
      moonAgeAtNoonHours: NaN,
      illuminationAtNoon: NaN,
      evening: emptyCrescent('evening'),
      morning: emptyCrescent('morning'),
    };
  }

  async searchCities(p: CitySearchParams): Promise<City[]> {
    const q = p.query.trim().toLowerCase();
    if (!q) return [];
    const hits = CITIES.filter((c) => c.name.toLowerCase().includes(q) || c.country.toLowerCase().includes(q));
    const prefix = (c: City) => (c.name.toLowerCase().startsWith(q) ? 0 : 1);
    hits.sort((a, b) => prefix(a) - prefix(b) || a.name.localeCompare(b.name));
    return structuredClone(hits.slice(0, Math.min(p.limit ?? 20, 100)));
  }

  async nearestCity(p: NearestCityParams): Promise<NearestCity | null> {
    // Equirectangular distance is plenty for a mock.
    let best: NearestCity | null = null;
    for (const c of CITIES) {
      const dx = (c.location.lonDeg - p.lonDeg) * Math.cos((p.latDeg * Math.PI) / 180);
      const dy = c.location.latDeg - p.latDeg;
      const km = Math.hypot(dx, dy) * 111.2;
      if (!best || km < best.distanceKm) best = { city: structuredClone(c), distanceKm: km };
    }
    return best;
  }

  async earliestSighting(p: EarliestParams): Promise<EarliestResult> {
    const c = criterion(p.criterionId);
    await delay(50);
    return {
      criterionId: c.id,
      criterionName: c.name,
      conjunction: { ...p.date, hour: 0, minute: 0, second: 0 },
      datesScanned: [],
      zones: c.zones.slice(0, -1).map((z, i) => ({ zone: i, ...z, first: null })),
      resolutionDeg: p.resolutionDeg ?? 2,
      timing: { prepareMs: 0, evaluateMs: 0, encodeMs: 0, totalMs: 0, threads: 1 },
    };
  }

  async basemap(p: BasemapParams): Promise<Basemap> {
    return { scale: p.scale ?? '110m', coastlines: { lines: [], points: 0 }, borders: { lines: [], points: 0 }, attribution: '' };
  }

  async loadDe440(_p: LoadDe440Params): Promise<EphemerisBackendInfo> {
    throw unsupported('DE440');
  }

  readonly sky = mockSkyApi();
}
