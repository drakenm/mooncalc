// Wire types of the Sky, Trajectory and Moon services.
//
// Mirrors crates/mooncalc-app/src/{sky/mod.rs, trajectory.rs, moon/mod.rs};
// keep them in step. Angles in degrees; position angles from north (or the
// zenith) through east, i.e. counter-clockwise as seen on the sky.

import type { CivilDate, Crescent, CrescentGeometry, Location, UtDateTime, Verdict } from './types';

// --- sky_scene, sky_stars -----------------------------------------------

export interface SkySceneParams {
  date: CivilDate;
  tzOffsetHours: number;
  location: Location;
  crescent: Crescent;
  /** Default "yallop". */
  criterionId?: string | null;
  ephemerisId?: string | null;
  /** Faintest star magnitude (default 3.5). */
  magnitudeLimit?: number | null;
}

/** The Sun and Moon at one instant of a sky scene. */
export interface SkySample {
  sunAltDeg: number;
  sunAzDeg: number;
  moonAltDeg: number;
  moonAzDeg: number;
  sunRaDeg: number;
  sunDecDeg: number;
  moonRaDeg: number;
  moonDecDeg: number;
  lstDeg: number;
  illuminatedFraction: number;
  /** χ, bright limb from north. */
  brightLimbDeg: number;
  /** q, parallactic angle. */
  parallacticDeg: number;
  /** χ − q, bright limb from the zenith. */
  limbZenithDeg: number;
  moonSemiDiameterArcmin: number;
  sunSemiDiameterArcmin: number;
  moonDistanceKm: number;
  arclDeg: number;
  arcvDeg: number;
  dazDeg: number;
  widthArcmin: number | null;
}

export interface StarDto {
  hr: number;
  name: string;
  label: string;
  vmag: number;
  bv: number;
  raDeg: number;
  decDeg: number;
  altDeg: number | null;
  azDeg: number | null;
}

export interface SkyScene {
  location: Location;
  date: CivilDate;
  tzOffsetHours: number;
  crescent: Crescent;
  ephemeris: string;
  criterionId: string;
  criterionName: string;
  evaluationTime: string;
  status: string;
  statusText: string;
  sunEvent: UtDateTime | null;
  moonEvent: UtDateTime | null;
  bestTime: UtDateTime | null;
  conjunction: UtDateTime | null;
  start: UtDateTime;
  end: UtDateTime;
  defaultTime: UtDateTime;
  geometry: CrescentGeometry | null;
  verdict: Verdict | null;
  /** Sample i is at samplesStart + i × stepMinutes. */
  samplesStart: UtDateTime;
  stepMinutes: number;
  samples: SkySample[];
  stars: StarDto[];
  obliquityDeg: number;
}

export interface SkyStarsParams {
  instant: UtDateTime;
  location?: Location | null;
  magnitudeLimit?: number | null;
}

export interface SkyStars {
  instant: UtDateTime;
  lstDeg: number | null;
  obliquityDeg: number;
  stars: StarDto[];
}

// --- trajectory_day -----------------------------------------------------

export interface TrajectoryParams {
  date: CivilDate;
  tzOffsetHours: number;
  location: Location;
  ephemerisId?: string | null;
}

export interface TrackSample {
  sunAltDeg: number;
  sunAzDeg: number;
  moonAltDeg: number;
  moonAzDeg: number;
  illuminatedFraction: number;
  elongationLonDeg: number;
  limbZenithDeg: number;
  moonDistanceKm: number;
  moonGeoDistanceKm: number;
  moonSemiDiameterArcmin: number;
}

export interface BodyDayEvents {
  rise: UtDateTime | null;
  set: UtDateTime | null;
  transit: UtDateTime | null;
  riseAzDeg: number | null;
  setAzDeg: number | null;
  transitAltDeg: number | null;
  /** "always-up" | "always-down" */
  circumpolar: string | null;
}

export interface TrajectoryTwilight {
  dawn: UtDateTime | null;
  dusk: UtDateTime | null;
}

export type PhaseCode = 'new' | 'first-quarter' | 'full' | 'last-quarter';

export interface PhaseEvent {
  phase: PhaseCode;
  time: UtDateTime;
}

export interface TrajectoryDay {
  location: Location;
  date: CivilDate;
  tzOffsetHours: number;
  ephemeris: string;
  dayStart: UtDateTime;
  stepMinutes: number;
  samples: TrackSample[];
  sun: BodyDayEvents;
  moon: BodyDayEvents;
  civil: TrajectoryTwilight;
  nautical: TrajectoryTwilight;
  astronomical: TrajectoryTwilight;
  phases: PhaseEvent[];
}

// --- moon_view, moon_features ----------------------------------------------

export interface MoonViewParams {
  instant: UtDateTime;
  location?: Location | null;
  ephemerisId?: string | null;
  crater?: string | null;
  /** Default true. */
  includeTrack?: boolean;
}

export interface LibrationDto {
  opticalLonDeg: number;
  opticalLatDeg: number;
  physicalLonDeg: number;
  physicalLatDeg: number;
  lonDeg: number;
  latDeg: number;
}

export interface MoonPhysicalDto {
  libration: LibrationDto;
  axisPositionAngleDeg: number;
  sunSelenographicLonDeg: number;
  sunSelenographicLatDeg: number;
  colongitudeDeg: number;
}

/** Disc drawing parameters: sub-observer and subsolar points, screen rotation of the lunar north pole (CCW from up). */
export interface DiscView {
  lDeg: number;
  bDeg: number;
  sunLonDeg: number;
  sunLatDeg: number;
  northUpRotationDeg: number;
  zenithUpRotationDeg: number | null;
}

export interface CraterSun {
  name: string;
  sunAltDeg: number;
  nextSunrise: UtDateTime | null;
  nextSunset: UtDateTime | null;
}

export interface LibrationTrack {
  newMoon: UtDateTime;
  fullMoon: UtDateTime;
  nextNewMoon: UtDateTime;
  /** [l, b] degrees, evenly spaced over the lunation. */
  points: [number, number][];
  now: [number, number];
  full: [number, number];
}

export interface MoonView {
  instant: UtDateTime;
  ephemeris: string;
  topocentric: boolean;
  geocentric: MoonPhysicalDto;
  topocentricPhysical: MoonPhysicalDto | null;
  disc: DiscView;
  illuminatedFraction: number;
  phaseAngleDeg: number;
  elongationDeg: number;
  brightLimbDeg: number;
  parallacticDeg: number | null;
  brightLimbZenithDeg: number | null;
  elongationLonDeg: number;
  phaseName: string;
  ageDays: number;
  lastNewMoon: UtDateTime;
  nextNewMoon: UtDateTime;
  nextFirstQuarter: UtDateTime;
  nextFullMoon: UtDateTime;
  nextLastQuarter: UtDateTime;
  distanceKm: number;
  geocentricDistanceKm: number;
  apparentDiameterArcmin: number;
  raDeg: number;
  decDeg: number;
  altDeg: number | null;
  azDeg: number | null;
  crater: CraterSun | null;
  librationTrack: LibrationTrack | null;
}

export interface CraterDto {
  name: string;
  latDeg: number;
  lonDeg: number;
  diameterKm: number;
}

export interface MareDto {
  name: string;
  latDeg: number;
  lonDeg: number;
  circular: boolean;
  /** Closed outline [lat0, lon0, lat1, lon1, ...] degrees. */
  outline: number[];
}

export interface MoonFeatures {
  craters: CraterDto[];
  maria: MareDto[];
  moonRadiusKm: number;
  attribution: string;
}

/** The Sky, Trajectory and Moon operations of a backend (`backend.sky`). */
export interface SkyApi {
  skyScene(params: SkySceneParams): Promise<SkyScene>;
  skyStars(params: SkyStarsParams): Promise<SkyStars>;
  trajectoryDay(params: TrajectoryParams): Promise<TrajectoryDay>;
  moonView(params: MoonViewParams): Promise<MoonView>;
  moonFeatures(): Promise<MoonFeatures>;
}
