// Desktop transport: Tauri 2 IPC. The only module allowed to import
// `@tauri-apps/api`. Command names and argument shapes match
// crates/mooncalc-tauri/src/commands.rs: each takes one `params` object.

import { invoke } from '@tauri-apps/api/core';

import { BackendError, type Backend } from './backend';
import { tauriSkyApi } from './tauri-sky';
import { tauriCalendarApi, tauriTablesApi } from './tauri-extras';
import type {
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

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    throw BackendError.from(err);
  }
}

export class TauriBackend implements Backend {
  readonly kind = 'tauri' as const;
  readonly calendar = tauriCalendarApi(call);
  readonly tables = tauriTablesApi(call);

  appInfo(): Promise<AppInfo> {
    return call('app_info');
  }

  criteriaList(): Promise<CriterionInfo[]> {
    return call('criteria_list');
  }

  computeVisibilityMap(params: MapParams): Promise<MapResult> {
    return call('compute_visibility_map', { params });
  }

  exportMapPng(params: ExportPngParams): Promise<PngImage> {
    return call('export_map_png', { params });
  }

  saveMapPng(params: ExportPngParams): Promise<SavedFile> {
    return call('save_map_png', { params });
  }

  locationReport(params: LocationReportParams): Promise<LocationReport> {
    return call('location_report', { params });
  }

  searchCities(params: CitySearchParams): Promise<City[]> {
    return call('search_cities', { params });
  }

  nearestCity(params: NearestCityParams): Promise<NearestCity | null> {
    return call('nearest_city', { params });
  }

  earliestSighting(params: EarliestParams): Promise<EarliestResult> {
    return call('earliest_sighting', { params });
  }

  basemap(params: BasemapParams): Promise<Basemap> {
    return call('basemap', { params });
  }

  loadDe440(params: LoadDe440Params): Promise<EphemerisBackendInfo> {
    return call('load_de440', { params });
  }

  readonly sky = tauriSkyApi(call);
}
