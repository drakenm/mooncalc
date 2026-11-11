// The transport-neutral backend contract.
//
// UI code talks to MoonCalc's Rust core only through this interface. Each
// transport implements it: TauriBackend (desktop, `invoke`), MockBackend
// (plain browser, synthetic data) and, later, WasmBackend (browser, the same
// Rust services compiled to wasm32). UI modules must never import a transport
// or `@tauri-apps/*` directly; they receive a Backend from `createBackend()`.

import type {
  ApiError,
  AppInfo,
  City,
  CitySearchParams,
  CriterionInfo,
  LocationReport,
  LocationReportParams,
  MapParams,
  MapResult,
} from './types';

export type BackendKind = 'tauri' | 'wasm' | 'mock';

export interface Backend {
  readonly kind: BackendKind;
  appInfo(): Promise<AppInfo>;
  criteriaList(): Promise<CriterionInfo[]>;
  computeVisibilityMap(params: MapParams): Promise<MapResult>;
  locationReport(params: LocationReportParams): Promise<LocationReport>;
  searchCities(params: CitySearchParams): Promise<City[]>;
}

/** Every backend method rejects with a BackendError. */
export class BackendError extends Error implements ApiError {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'BackendError';
    this.code = code;
  }

  /** Normalise whatever a transport threw into a BackendError. */
  static from(err: unknown): BackendError {
    if (err instanceof BackendError) return err;
    if (typeof err === 'object' && err !== null && 'code' in err && 'message' in err) {
      const e = err as { code: unknown; message: unknown };
      return new BackendError(String(e.code), String(e.message));
    }
    if (err instanceof Error) return new BackendError('transport', err.message);
    return new BackendError('transport', String(err));
  }
}
