// Wire types of the ephemeris tables service (the Ephemeris tab).
//
// Mirrors crates/mooncalc-app/src/tables/dto.rs; keep the two in step.
// Angles in degrees, times in UT.

import type { CivilDate, Location, UtDateTime } from './types';

// --- ephemeris_table --------------------------------------------------------

export interface EphemerisTableParams {
  start: UtDateTime;
  /** 0.1 ..= 525600. */
  stepMinutes: number;
  /** 1 ..= 10000. */
  count: number;
  location?: Location | null;
  ephemerisId?: string | null;
  /** Zone of the rows' `offsetHours` (IANA, else the fixed offset). */
  tzOffsetHours?: number;
  timeZone?: string | null;
}

export interface TopocentricRow {
  /** With standard refraction. */
  altDeg: number;
  altAirlessDeg: number;
  azDeg: number;
  raDeg: number;
  decDeg: number;
  distanceKm: number;
  semidiameterArcmin: number;
}

export interface BodyRow {
  raDeg: number;
  decDeg: number;
  lonDeg: number;
  latDeg: number;
  distanceKm: number;
  semidiameterArcmin: number;
  parallaxArcmin: number;
  topocentric: TopocentricRow | null;
}

export interface LibrationRow {
  lonDeg: number;
  latDeg: number;
  axisPositionAngleDeg: number;
  colongitudeDeg: number;
  sunLatDeg: number;
}

export interface EphemerisRow {
  time: UtDateTime;
  jdUt: number;
  jdTt: number;
  sun: BodyRow;
  moon: BodyRow;
  elongationDeg: number;
  phaseAngleDeg: number;
  illuminatedFraction: number;
  brightLimbDeg: number;
  topocentricElongationDeg: number | null;
  moonAgeDays: number;
  conjunction: UtDateTime;
  libration: LibrationRow;
}

export interface EphemerisTable {
  rows: EphemerisRow[];
  ephemeris: string;
}

// --- rise_set_month ---------------------------------------------------------

export interface RiseSetParams {
  year: number;
  month: number;
  /** Fixed offset from UT, hours, used when `timeZone` is absent. */
  tzOffsetHours: number;
  /** IANA zone whose rules give the offset on each date; wins over `tzOffsetHours`. */
  timeZone?: string | null;
  location: Location;
  ephemerisId?: string | null;
}

export interface AngledEvent {
  time: UtDateTime;
  /** Azimuth (rise/set) or refracted altitude (transit). */
  angleDeg: number;
}

export interface BodyDay {
  rise: AngledEvent | null;
  transit: AngledEvent | null;
  set: AngledEvent | null;
  circumpolar: 'always-up' | 'always-down' | null;
}

export interface TwilightTimes {
  dawn: UtDateTime | null;
  dusk: UtDateTime | null;
}

export interface RiseSetDay {
  date: CivilDate;
  sun: BodyDay;
  moon: BodyDay;
  civil: TwilightTimes;
  nautical: TwilightTimes;
  astronomical: TwilightTimes;
}

export interface RiseSetMonth {
  year: number;
  month: number;
  /** The offset at local civil noon (each time carries its own `offsetHours`). */
  tzOffsetHours: number;
  /** The IANA zone used, if any. */
  timeZone: string | null;
  location: Location;
  days: RiseSetDay[];
  ephemeris: string;
}

// --- year lists -------------------------------------------------------------

export interface YearParams {
  year: number;
  tzOffsetHours?: number;
  /** IANA zone: the year runs between its local midnights. */
  timeZone?: string | null;
  ephemerisId?: string | null;
}

export interface Lunation {
  k: number;
  newMoon: UtDateTime;
  lengthDays: number;
  /** New, first quarter, full, last quarter (where inside the year). */
  phases: [UtDateTime | null, UtDateTime | null, UtDateTime | null, UtDateTime | null];
  agesDays: [number | null, number | null, number | null, number | null];
}

export interface LunarPhasesYear {
  year: number;
  lunations: Lunation[];
  ephemeris: string;
}

export interface Apsis {
  kind: 'perigee' | 'apogee';
  time: UtDateTime;
  distanceKm: number;
  parallaxArcsec: number;
  diameterArcmin: number;
}

export interface NodePassage {
  kind: 'ascending' | 'descending';
  time: UtDateTime;
}

export interface DeclinationExtreme {
  kind: 'north' | 'south';
  time: UtDateTime;
  decDeg: number;
}

export interface ApsidesNodesYear {
  year: number;
  apsides: Apsis[];
  nodes: NodePassage[];
  declinations: DeclinationExtreme[];
  ephemeris: string;
}

export interface SeasonEvent {
  kind: 'march-equinox' | 'june-solstice' | 'september-equinox' | 'december-solstice';
  name: string;
  time: UtDateTime;
  equationOfTimeMin: number;
}

export interface EquationOfTimePoint {
  date: CivilDate;
  minutes: number;
}

export interface SeasonsYear {
  year: number;
  seasons: SeasonEvent[];
  equationOfTime: EquationOfTimePoint[];
  ephemeris: string;
}

// --- eclipses ---------------------------------------------------------------

export type EclipseKindFilter = 'any' | 'solar' | 'lunar';

export interface EclipsesParams {
  firstYear: number;
  lastYear: number;
  filter?: EclipseKindFilter;
  location?: Location | null;
  ephemerisId?: string | null;
  /** Zone of the times' `offsetHours` (IANA, else the fixed offset). */
  tzOffsetHours?: number;
  timeZone?: string | null;
}

export interface SolarContact {
  time: UtDateTime;
  sunAltDeg: number;
  sunAzDeg: number;
}

export interface LocalSolarEclipse {
  kind: 'partial' | 'annular' | 'total';
  c1: SolarContact;
  c2: SolarContact | null;
  max: SolarContact;
  c3: SolarContact | null;
  c4: SolarContact;
  magnitude: number;
  obscuration: number;
  moonSunRatio: number;
  durationMin: number;
  centralDurationMin: number | null;
  visible: boolean;
}

export interface SolarEclipseDetails {
  central: boolean;
  greatestLatDeg: number;
  greatestLonDeg: number;
  sunAltDeg: number;
  local: LocalSolarEclipse | null;
}

export interface LunarContact {
  code: 'P1' | 'U1' | 'U2' | 'max' | 'U3' | 'U4' | 'P4';
  label: string;
  time: UtDateTime;
  moonAltDeg: number | null;
  aboveHorizon: boolean | null;
}

export interface LunarEclipseDetails {
  penumbralMagnitude: number;
  umbralMagnitude: number;
  contacts: LunarContact[];
  penumbralDurationMin: number | null;
  partialDurationMin: number | null;
  totalDurationMin: number | null;
  zenithLatDeg: number;
  zenithLonDeg: number;
  visible: boolean | null;
}

export interface EclipseInfo {
  body: 'solar' | 'lunar';
  kind: string;
  label: string;
  lunation: number;
  greatest: UtDateTime;
  gamma: number;
  magnitude: number;
  solar: SolarEclipseDetails | null;
  lunar: LunarEclipseDetails | null;
}

export interface EclipseList {
  firstYear: number;
  lastYear: number;
  location: Location | null;
  eclipses: EclipseInfo[];
  ephemeris: string;
}

/** The ephemeris-table operations of a Backend (`backend.tables`). */
export interface TablesApi {
  ephemerisTable(params: EphemerisTableParams): Promise<EphemerisTable>;
  riseSetMonth(params: RiseSetParams): Promise<RiseSetMonth>;
  lunarPhases(params: YearParams): Promise<LunarPhasesYear>;
  apsidesNodes(params: YearParams): Promise<ApsidesNodesYear>;
  eclipses(params: EclipsesParams): Promise<EclipseList>;
  seasons(params: YearParams): Promise<SeasonsYear>;
}
