// Wire types shared by every backend transport.
//
// Mirrors crates/mooncalc-app/src/dto.rs; keep the two in step. Conventions:
// camelCase fields, unit suffixes in names (`latDeg`, `elevationM`),
// longitude east-positive, all date-times in UT, binary payloads as base64.

/** Zone byte marking a map cell with no evaluation. */
export const NO_DATA = 255;
/** Quantised value marking a cell without a criterion value. */
export const NO_VALUE = -32768;

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
  /** "invalid-params", "unknown-criterion", "ephemeris-unavailable", "cancelled", "internal". */
  code: string;
  message: string;
}

// --- app_info, ephemerides ------------------------------------------------

export interface EphemerisBackendInfo {
  /** "meeus", "meeus-full" or "de440". */
  id: string;
  name: string;
  description: string;
  /** Compiled into this build. */
  available: boolean;
  needsFile: boolean;
  /** Usable now (DE440 once a file is loaded). */
  ready: boolean;
  file: string | null;
  detail: string | null;
}

export interface AppInfo {
  name: string;
  version: string;
  /** Hosting runtime, e.g. "tauri 2.12.5", "wasm", "mock". */
  runtime: string;
  ephemerisBackends: EphemerisBackendInfo[];
  defaultEphemeris: string;
  attributions: string[];
}

export interface LoadDe440Params {
  path: string;
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
  /** e.g. "Best time (sunset + 4/9 lag)". */
  evaluationTime: string;
  /** Descending: zone i is value >= thresholds[i]; the last zone anything lower. */
  thresholds: number[] | null;
  /** Most visible first; a map zone byte indexes this list. */
  zones: ZoneLegendEntry[];
}

// --- compute_visibility_map -----------------------------------------------

export interface Bounds {
  latMinDeg: number;
  latMaxDeg: number;
  lonMinDeg: number;
  lonMaxDeg: number;
}

export interface MapParams {
  date: CivilDate;
  criterionId: string;
  crescent: Crescent;
  /** Grid spacing, degrees (0.1..10). 0.25 gives a 1440 × 720 grid. */
  resolutionDeg: number;
  /** Omitted: the whole world. */
  bounds?: Bounds | null;
  ephemerisId?: string | null;
  /** Send the value field (default true). */
  includeValues?: boolean;
}

/** Equirectangular grid; row 0 is north, column 0 is west. */
export interface GridSpec extends Bounds {
  rows: number;
  cols: number;
}

/** Value per cell = offset + scale × q, q a little-endian int16 (NO_VALUE = none). */
export interface ValueField {
  scale: number;
  offset: number;
  noValue: number;
  /** Base64 of rows × cols int16. */
  data: string;
}

export interface StatusCounts {
  crescent: number;
  beforeConjunction: number;
  moonSetsFirst: number;
  moonAlwaysUp: number;
  moonAlwaysDown: number;
  midnightSun: number;
  polarNight: number;
  sunAltitudeNotReached: number;
  /** Cells whose evening falls outside the ephemeris coverage. */
  outsideEphemeris: number;
}

export interface BestCell {
  latDeg: number;
  lonDeg: number;
  zone: number;
  value: number | null;
}

export interface MapTiming {
  prepareMs: number;
  evaluateMs: number;
  encodeMs: number;
  totalMs: number;
  threads: number;
}

export interface MapResult {
  criterionId: string;
  criterionName: string;
  date: CivilDate;
  crescent: Crescent;
  grid: GridSpec;
  /** Base64 of rows × cols zone bytes, row-major from the north-west corner. */
  cells: string;
  /** Byte value of cells without an evaluation (255). */
  noData: number;
  legend: ZoneLegendEntry[];
  values: ValueField | null;
  thresholds: number[] | null;
  /** Cells per legend zone. */
  zoneCounts: number[];
  statusCounts: StatusCounts;
  /** The geocentric new Moon nearest the date. */
  conjunction: UtDateTime;
  evaluationTime: string;
  /** "Each location is shown on its own local evening of <date>; the date line is the 180° meridian (HMNAO convention)." */
  dateConvention: string;
  best: BestCell | null;
  ephemeris: string;
  timing: MapTiming;
}

export interface MapMarker {
  location: Location;
  label?: string | null;
}

export interface ExportPngParams {
  map: MapParams;
  /** 256..8192, default 2048. */
  widthPx?: number | null;
  legend?: boolean;
  marker?: MapMarker | null;
}

export interface PngImage {
  width: number;
  height: number;
  /** Base64 of the PNG file. */
  png: string;
  fileName: string;
}

/** Desktop only: where save_map_png wrote the image. */
export interface SavedFile {
  path: string;
  width: number;
  height: number;
}

// --- location_report ------------------------------------------------------

export interface LocationReportParams {
  /** Local civil date in the tzOffsetHours zone. */
  date: CivilDate;
  tzOffsetHours: number;
  location: Location;
  ephemerisId?: string | null;
}

export interface DayEvents {
  sunrise: UtDateTime | null;
  sunset: UtDateTime | null;
  solarNoon: UtDateTime | null;
  moonrise: UtDateTime | null;
  moonset: UtDateTime | null;
  moonTransit: UtDateTime | null;
  civilDawn: UtDateTime | null;
  civilDusk: UtDateTime | null;
  nauticalDawn: UtDateTime | null;
  nauticalDusk: UtDateTime | null;
}

/** Mirrors mooncalc_visibility::CrescentGeometry (degrees). */
export interface CrescentGeometry {
  arclDeg: number;
  arclGeoDeg: number;
  arcvDeg: number;
  arcvTopoDeg: number;
  dazDeg: number;
  widthTopoArcmin: number;
  widthGeoArcmin: number;
  semiDiameterTopoArcmin: number;
  moonAltDeg: number;
  moonAltAirlessDeg: number;
  sunAltDeg: number;
  moonAzDeg: number;
  sunAzDeg: number;
  /** null when the Moon does not set (rise) near the Sun event. */
  lagMinutes: number | null;
  ageHours: number;
  illuminatedFraction: number;
  moonDistanceKm: number;
}

export interface Verdict {
  zone: number;
  code: string;
  description: string;
  rgb: [number, number, number];
  value: number | null;
}

export interface CriterionVerdict {
  criterionId: string;
  name: string;
  evaluationTime: string;
  evaluatedAt: UtDateTime | null;
  status: string;
  verdict: Verdict | null;
}

export interface CrescentReport {
  crescent: Crescent;
  /** "crescent", "before-conjunction", "moon-sets-first", ... */
  status: string;
  statusText: string;
  sunEvent: UtDateTime | null;
  moonEvent: UtDateTime | null;
  bestTime: UtDateTime | null;
  /** New Moon nearest the geometry's instant (nearest local noon without geometry). */
  conjunction: UtDateTime | null;
  /** At the best time, or at the Sun event when the Moon sets first or the best time is undefined. */
  geometry: CrescentGeometry | null;
  /** The instant of `geometry`. */
  geometryAt: UtDateTime | null;
  /** `geometry` is at the Sun event because the best time is undefined (moon-always-up). */
  geometryAtSunEvent: boolean;
  criteria: CriterionVerdict[];
}

export interface LocationReport {
  location: Location;
  date: CivilDate;
  tzOffsetHours: number;
  ephemeris: string;
  day: DayEvents;
  moonAgeAtNoonHours: number;
  illuminationAtNoon: number;
  evening: CrescentReport;
  morning: CrescentReport;
  /** Set when the world map for `date` shows another evening at this longitude. */
  mapDateNote: MapDateNote | null;
}

/** Which world-map date shows a report's evening (mooncalc_app::dto::MapDateNote). */
export interface MapDateNote {
  mapDate: CivilDate;
  /** mapDate − date, days: −1 east of 180° in UTC+13/+14. */
  dayShift: number;
  text: string;
}

// --- atlas ----------------------------------------------------------------

export interface CitySearchParams {
  query: string;
  limit?: number | null;
}

export interface NearestCityParams {
  latDeg: number;
  lonDeg: number;
}

export interface City {
  name: string;
  admin1: string | null;
  country: string;
  countryCode: string;
  location: Location;
  /** IANA zone, e.g. "Asia/Riyadh". */
  timeZone: string | null;
  stdOffsetHours: number;
  dstOffsetHours: number;
  population: number | null;
}

export interface NearestCity {
  city: City;
  distanceKm: number;
}

// --- earliest_sighting ----------------------------------------------------

export interface EarliestParams {
  /** Any date near the new Moon of interest. */
  date: CivilDate;
  criterionId: string;
  /** 0.25..10, default 2. */
  resolutionDeg?: number | null;
  /** 1..10, default 4. */
  evenings?: number | null;
  ephemerisId?: string | null;
}

export interface FirstSighting {
  date: CivilDate;
  latDeg: number;
  lonDeg: number;
  value: number | null;
  cells: number;
  nearestCity: string | null;
}

export interface EarliestZone {
  zone: number;
  code: string;
  description: string;
  rgb: [number, number, number];
  first: FirstSighting | null;
}

export interface EarliestResult {
  criterionId: string;
  criterionName: string;
  conjunction: UtDateTime;
  datesScanned: CivilDate[];
  zones: EarliestZone[];
  resolutionDeg: number;
  timing: MapTiming;
}

// --- basemap --------------------------------------------------------------

export type BasemapScale = '110m' | '50m';

export interface BasemapParams {
  scale?: BasemapScale;
  toleranceDeg?: number | null;
}

/** Each line is [lon0, lat0, lon1, lat1, ...] in degrees. */
export interface PolylineSet {
  lines: number[][];
  points: number;
}

export interface Basemap {
  scale: BasemapScale;
  coastlines: PolylineSet;
  borders: PolylineSet;
  attribution: string;
}

export * from './types-sky';
export * from './types-calendar';
export * from './types-tables';
