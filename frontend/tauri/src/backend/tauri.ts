// Desktop transport: Tauri 2 IPC. The only module allowed to import
// `@tauri-apps/api`. Command names and argument shapes match
// crates/mooncalc-tauri/src/commands.rs: each takes one `params` object.

import { invoke } from '@tauri-apps/api/core';

import { BackendError, type Backend } from './backend';
import type {
  AppInfo,
  City,
  CitySearchParams,
  CriterionInfo,
  LocationReport,
  LocationReportParams,
  MapParams,
  MapResult,
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

  appInfo(): Promise<AppInfo> {
    return call('app_info');
  }

  criteriaList(): Promise<CriterionInfo[]> {
    return call('criteria_list');
  }

  computeVisibilityMap(params: MapParams): Promise<MapResult> {
    return call('compute_visibility_map', { params });
  }

  locationReport(params: LocationReportParams): Promise<LocationReport> {
    return call('location_report', { params });
  }

  searchCities(params: CitySearchParams): Promise<City[]> {
    return call('search_cities', { params });
  }
}
