// The transport-neutral backend contract.
//
// UI code talks to MoonCalc's Rust core only through this interface. Each
// transport implements it: TauriBackend (desktop, `invoke`), MockBackend
// (plain browser, synthetic data) and, later, WasmBackend (browser, the same
// mooncalc-app services compiled to wasm32). UI modules must never import a
// transport or `@tauri-apps/*` directly; they receive a Backend from
// `createBackend()`.

import type {
  ApiError,
  AppInfo,
  Basemap,
  BasemapParams,
  City,
  CitySearchParams,
  CriterionInfo,
  EarliestParams,
  EarliestResult,
  EphemerisBackendInfo,
  ExportPngParams,
  LoadDe440Params,
  LocationReport,
  LocationReportParams,
  MapParams,
  MapResult,
  NearestCity,
  NearestCityParams,
  PngImage,
  SavedFile,
} from './types';
import type { SkyApi } from './types-sky';

export type BackendKind = 'tauri' | 'wasm' | 'mock';

export interface Backend {
  readonly kind: BackendKind;
  appInfo(): Promise<AppInfo>;
  criteriaList(): Promise<CriterionInfo[]>;
  computeVisibilityMap(params: MapParams): Promise<MapResult>;
  exportMapPng(params: ExportPngParams): Promise<PngImage>;
  /** Desktop transports only: render and save to the Downloads folder. */
  saveMapPng?(params: ExportPngParams): Promise<SavedFile>;
  locationReport(params: LocationReportParams): Promise<LocationReport>;
  searchCities(params: CitySearchParams): Promise<City[]>;
  nearestCity(params: NearestCityParams): Promise<NearestCity | null>;
  earliestSighting(params: EarliestParams): Promise<EarliestResult>;
  basemap(params: BasemapParams): Promise<Basemap>;
  loadDe440(params: LoadDe440Params): Promise<EphemerisBackendInfo>;
  /** Sky, Trajectory and Moon tabs. */
  readonly sky: SkyApi;
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
