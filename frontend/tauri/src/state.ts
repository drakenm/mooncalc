// Application state shared by the sidebar and the tabs.

import type { CivilDate, Crescent, Location, UtDateTime } from './backend';
import { localDateOf, noonOffset, tzOf } from './zone';

export interface AppState {
  /** Date and time of interest, UT (seconds kept at 0 by the UI). */
  instant: UtDateTime;
  /** Fixed offset of local time from UT, hours, used when `timeZone` is null. */
  tzOffsetHours: number;
  /** IANA time zone, whose rules (DST, history) give the offset at each
   * instant; null for the fixed offset. Choosing a city sets it. Its offset
   * table is loaded (`zone.ts`) before the state names it. */
  timeZone: string | null;
  location: Location;
  /** Name of the selected city, cleared when coordinates are edited. */
  locationName: string | null;
  criterionId: string;
  crescent: Crescent;
  ephemerisId: string;
  /** Path of the DE440/DE440s .bsp file last loaded (desktop only). */
  de440Path: string;
  /** Map grid spacing, degrees. */
  mapResolutionDeg: number;
}

export type Listener = (state: AppState, prev: AppState) => void;

const STORAGE_KEY = 'mooncalc.state.v1';

function nowUt(): UtDateTime {
  const d = new Date();
  return {
    year: d.getUTCFullYear(),
    month: d.getUTCMonth() + 1,
    day: d.getUTCDate(),
    hour: d.getUTCHours(),
    minute: d.getUTCMinutes(),
    second: 0,
  };
}

export function defaultState(): AppState {
  return {
    instant: nowUt(),
    tzOffsetHours: 3,
    timeZone: 'Asia/Riyadh',
    location: { latDeg: 21.4225, lonDeg: 39.8262, elevationM: 277 },
    locationName: 'Makkah, Saudi Arabia',
    criterionId: 'yallop',
    crescent: 'evening',
    ephemerisId: 'meeus',
    de440Path: '',
    mapResolutionDeg: 0.5,
  };
}

/** A minimal observable store. Persists to localStorage when available. */
export class Store {
  private state: AppState;
  private readonly listeners = new Set<Listener>();

  constructor() {
    this.state = { ...defaultState(), ...Store.load() };
  }

  get(): AppState {
    return this.state;
  }

  update(patch: Partial<AppState>): void {
    const prev = this.state;
    this.state = { ...prev, ...patch };
    Store.save(this.state);
    for (const l of this.listeners) l(this.state, prev);
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Restores everything except the instant, which starts at "now". */
  private static load(): Partial<AppState> {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return {};
      const saved = JSON.parse(raw) as Partial<AppState>;
      delete saved.instant;
      // Saved before zones existed: keep the fixed offset.
      if (!('timeZone' in saved)) saved.timeZone = null;
      return saved;
    } catch {
      return {};
    }
  }

  private static save(state: AppState): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch {
      // Storage unavailable (private mode, quota): state is just not remembered.
    }
  }
}

/**
 * The local civil date of the selected instant (UT shifted by the zone's
 * offset then): the date whose evening the map and the report show.
 */
export function localDate(s: Pick<AppState, 'instant' | 'tzOffsetHours' | 'timeZone'>): CivilDate {
  return localDateOf(tzOf(s), s.instant);
}

/**
 * The time-zone fields of a request about local date `date`: the IANA zone,
 * and as the fixed fallback the zone's offset at civil noon of that date.
 */
export function zoneParams(s: Pick<AppState, 'tzOffsetHours' | 'timeZone'>, date: CivilDate): { tzOffsetHours: number; timeZone: string | null } {
  return { tzOffsetHours: noonOffset(tzOf(s), date), timeZone: s.timeZone };
}

/** Shallow equality on the listed keys, for "did anything relevant change?". */
export function changed<K extends keyof AppState>(a: AppState, b: AppState, keys: K[]): boolean {
  return keys.some((k) => JSON.stringify(a[k]) !== JSON.stringify(b[k]));
}
