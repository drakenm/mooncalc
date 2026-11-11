// In-browser stand-in backend for `npm run dev` outside Tauri.
//
// Produces the same shapes as the Rust stubs (and the same synthetic map
// pattern as crates/mooncalc-tauri/src/synthetic.rs) so the UI can be
// developed in any browser. No astronomy happens here.

import { BackendError, type Backend } from './backend';
import { bytesToBase64 } from './codec';
import {
  NO_DATA,
  type AppInfo,
  type City,
  type CitySearchParams,
  type CriterionInfo,
  type LocationReport,
  type LocationReportParams,
  type MapParams,
  type MapResult,
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
    zones: [
      zone('A', 'Easily visible (q > +0.216)', [46, 160, 67]),
      zone('B', 'Visible under perfect conditions (q > -0.014)', [154, 209, 75]),
      zone('C', 'May need optical aid to find the crescent (q > -0.160)', [242, 208, 59]),
      zone('D', 'Will need optical aid (q > -0.232)', [240, 140, 46]),
      zone('E', 'Not visible with a telescope (q > -0.293)', [214, 60, 60]),
      zone('F', 'Not visible, below the Danjon limit (q <= -0.293)', [120, 120, 130]),
    ],
  },
  {
    id: 'odeh',
    name: 'Odeh (2004)',
    reference: 'Odeh, M. Sh. (2004), Experimental Astronomy 18, 39-64.',
    zones: [
      zone('A', 'Visible by naked eye (V >= 5.65)', [46, 160, 67]),
      zone('B', 'Visible by optical aid, could be seen by naked eye (V >= 2.00)', [154, 209, 75]),
      zone('C', 'Visible by optical aid only (V >= -0.96)', [240, 140, 46]),
      zone('D', 'Not visible even with optical aid (V < -0.96)', [214, 60, 60]),
    ],
  },
];

const CITIES: City[] = [
  ['Makkah', 'Saudi Arabia', 21.4225, 39.8262, 277, 'Asia/Riyadh'],
  ['Madinah', 'Saudi Arabia', 24.4686, 39.6142, 608, 'Asia/Riyadh'],
  ['Cairo', 'Egypt', 30.0444, 31.2357, 23, 'Africa/Cairo'],
  ['Istanbul', 'Turkey', 41.0082, 28.9784, 39, 'Europe/Istanbul'],
  ['Karachi', 'Pakistan', 24.8607, 67.0011, 10, 'Asia/Karachi'],
  ['Kuala Lumpur', 'Malaysia', 3.139, 101.6869, 56, 'Asia/Kuala_Lumpur'],
  ['Jakarta', 'Indonesia', -6.2088, 106.8456, 8, 'Asia/Jakarta'],
  ['London', 'United Kingdom', 51.5074, -0.1278, 11, 'Europe/London'],
  ['New York', 'United States', 40.7128, -74.006, 10, 'America/New_York'],
  ['Cape Town', 'South Africa', -33.9249, 18.4241, 15, 'Africa/Johannesburg'],
  ['Sydney', 'Australia', -33.8688, 151.2093, 58, 'Australia/Sydney'],
].map(([name, country, latDeg, lonDeg, elevationM, timeZone]) => ({
  name: name as string,
  admin1: null,
  country: country as string,
  location: { latDeg: latDeg as number, lonDeg: lonDeg as number, elevationM: elevationM as number },
  timeZone: timeZone as string,
  population: null,
}));

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function criterion(id: string): CriterionInfo {
  const c = CRITERIA.find((x) => x.id === id);
  if (!c) throw new BackendError('unknown-criterion', `unknown criterion id "${id}"`);
  return c;
}

export class MockBackend implements Backend {
  readonly kind = 'mock' as const;

  async appInfo(): Promise<AppInfo> {
    return {
      name: 'MoonCalc',
      version: '0.1.0',
      runtime: 'mock (browser)',
      ephemerisBackends: [
        { id: 'meeus', name: 'Meeus / ELP-2000/82', description: 'Built-in analytical theory.', available: true },
        { id: 'de440', name: 'JPL DE440', description: 'Requires the de440 build feature.', available: false },
      ],
      defaultEphemeris: 'meeus',
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
    // Same pattern as synthetic::visibility_grid in Rust.
    const n = p.date.year * 372 + p.date.month * 31 + p.date.day;
    const lon0 = (((n * 47) % 360) + 360) % 360 - 180;
    const lat0 = 15 * Math.sin(n * 0.21);
    const peakLon = p.crescent === 'evening' ? lon0 - 90 : lon0 + 90;
    const maxZone = c.zones.length - 1;
    const dLat = 180 / rows;
    const dLon = 360 / cols;
    for (let r = 0; r < rows; r++) {
      const lat = 90 - (r + 0.5) * dLat;
      if (Math.abs(lat) > 62) continue;
      const latTerm = ((lat - lat0) / 40) ** 2;
      for (let col = 0; col < cols; col++) {
        const lon = -180 + (col + 0.5) * dLon;
        const s = Math.cos(((lon - peakLon) * Math.PI) / 180) - latTerm;
        if (s < -0.9) continue;
        cells[r * cols + col] = Math.min(maxZone, Math.max(0, Math.floor((0.8 - s) / 0.3)));
      }
    }
    const computeMs = performance.now() - start;
    await delay(50);
    return {
      criterionId: c.id,
      date: p.date,
      crescent: p.crescent,
      grid: { latMinDeg: -90, latMaxDeg: 90, lonMinDeg: -180, lonMaxDeg: 180, rows, cols },
      cells: bytesToBase64(cells),
      noData: NO_DATA,
      legend: structuredClone(c.zones),
      computeMs,
      synthetic: true,
    };
  }

  async locationReport(p: LocationReportParams): Promise<LocationReport> {
    const c = criterion(p.criterionId);
    await delay(30);
    return {
      location: p.location,
      criterionId: c.id,
      crescent: p.crescent,
      sunEvent: null,
      moonEvent: null,
      bestTime: null,
      conjunction: null,
      geometry: null,
      verdict: null,
      notes: ['Mock backend: no computation in the browser preview.'],
      synthetic: true,
    };
  }

  async searchCities(p: CitySearchParams): Promise<City[]> {
    const q = p.query.trim().toLowerCase();
    if (!q) return [];
    const hits = CITIES.filter(
      (c) => c.name.toLowerCase().includes(q) || c.country.toLowerCase().includes(q),
    );
    const prefix = (c: City) => (c.name.toLowerCase().startsWith(q) ? 0 : 1);
    hits.sort((a, b) => prefix(a) - prefix(b) || a.name.localeCompare(b.name));
    return structuredClone(hits.slice(0, Math.min(p.limit ?? 20, 100)));
  }
}
