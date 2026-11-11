// Application state shared by the sidebar and the tabs.

import type { Crescent, Location, UtDateTime } from './backend';

export interface AppState {
  /** Date and time of interest, UT (seconds kept at 0 by the UI). */
  instant: UtDateTime;
  /** Local time offset from UT, hours; display only. */
  tzOffsetHours: number;
  location: Location;
  /** Name of the selected city, cleared when coordinates are edited. */
  locationName: string | null;
  criterionId: string;
  crescent: Crescent;
  ephemerisId: string;
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
    location: { latDeg: 21.4225, lonDeg: 39.8262, elevationM: 277 },
    locationName: 'Makkah, Saudi Arabia',
    criterionId: 'yallop',
    crescent: 'evening',
    ephemerisId: 'meeus',
    mapResolutionDeg: 0.25,
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

/** Shallow equality on the listed keys, for "did anything relevant change?". */
export function changed<K extends keyof AppState>(a: AppState, b: AppState, keys: K[]): boolean {
  return keys.some((k) => JSON.stringify(a[k]) !== JSON.stringify(b[k]));
}
