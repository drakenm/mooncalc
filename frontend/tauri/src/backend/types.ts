// Wire types shared by every backend transport.
//
// Mirrors crates/mooncalc-tauri/src/dto.rs (and later mooncalc-app); keep the
// two in step. Conventions: camelCase fields, unit suffixes in names (`latDeg`,
// `elevationM`), longitude east-positive, all date-times in UT.

/** Zone byte marking a map cell with no evaluation. */
export const NO_DATA = 255;

export interface Location {
  /** Degrees, north-positive. */
  latDeg: number;
  /** Degrees, east-positive. */
  lonDeg: number;
  /** Metres above sea level. */
  elevationM: number;
}

export interface CivilDate {
  year: number;
  /** 1..12 */
  month: number;
  /** 1..31 */
  day: number;
}

/** A date and time in UT. */
export interface UtDateTime extends CivilDate {
  hour: number;
  minute: number;
  second: number;
}

export type Crescent = 'evening' | 'morning';

/** The rejection value of every backend call. */
export interface ApiError {
  /** Stable code, e.g. "invalid-params", "unknown-criterion", "internal". */
  code: string;
  message: string;
}

// --- app_info -------------------------------------------------------------

export interface EphemerisBackendInfo {
  id: string;
  name: string;
  description: string;
  available: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  /** Hosting runtime, e.g. "tauri 2.12.5", "wasm", "mock". */
  runtime: string;
  ephemerisBackends: EphemerisBackendInfo[];
  defaultEphemeris: string;
}

// --- criteria_list --------------------------------------------------------

export interface ZoneLegendEntry {
  code: string;
  description: string;
  /** sRGB */
  rgb: [number, number, number];
}

export interface CriterionInfo {
  id: string;
  name: string;
  reference: string;
  /** Most visible first; a map zone byte indexes this list. */
  zones: ZoneLegendEntry[];
}

// --- compute_visibility_map -----------------------------------------------

export interface MapParams {
  date: CivilDate;
  criterionId: string;
  crescent: Crescent;
  /** Grid spacing, degrees (0.1..10). 0.25 gives a 1440 × 720 grid. */
  resolutionDeg: number;
  ephemerisId?: string | null;
}

/** Equirectangular grid; row 0 is north, column 0 is west. */
export interface GridSpec {
  latMinDeg: number;
  latMaxDeg: number;
  lonMinDeg: number;
  lonMaxDeg: number;
  rows: number;
  cols: number;
}

export interface MapResult {
  criterionId: string;
  date: CivilDate;
  crescent: Crescent;
  grid: GridSpec;
  /** Base64 of rows × cols zone bytes, row-major from the north-west corner. */
  cells: string;
  /** Byte value of cells without an evaluation (255). */
  noData: number;
  legend: ZoneLegendEntry[];
  computeMs: number;
  synthetic: boolean;
}

// --- location_report ------------------------------------------------------

export interface LocationReportParams {
  instant: UtDateTime;
  tzOffsetHours: number;
  location: Location;
  criterionId: string;
  crescent: Crescent;
  ephemerisId?: string | null;
}

/** Mirrors mooncalc_visibility::CrescentGeometry (degrees). */
export interface CrescentGeometry {
  arclDeg: number;
  arclGeoDeg: number;
  arcvDeg: number;
  dazDeg: number;
  widthTopoArcmin: number;
  widthGeoArcmin: number;
  semiDiameterTopoArcmin: number;
  moonAltDeg: number;
  moonAltAirlessDeg: number;
  sunAltDeg: number;
  moonAzDeg: number;
  sunAzDeg: number;
  lagMinutes: number;
  ageHours: number;
  illuminatedFraction: number;
  moonDistanceKm: number;
}

export interface Verdict {
  zone: number;
  code: string;
  description: string;
  value: number | null;
}

export interface LocationReport {
  location: Location;
  criterionId: string;
  crescent: Crescent;
  sunEvent: UtDateTime | null;
  moonEvent: UtDateTime | null;
  bestTime: UtDateTime | null;
  conjunction: UtDateTime | null;
  geometry: CrescentGeometry | null;
  verdict: Verdict | null;
  notes: string[];
  synthetic: boolean;
}

// --- search_cities --------------------------------------------------------

export interface CitySearchParams {
  query: string;
  limit?: number | null;
}

export interface City {
  name: string;
  admin1: string | null;
  country: string;
  location: Location;
  /** IANA zone, e.g. "Asia/Riyadh". */
  timeZone: string | null;
  population: number | null;
}
