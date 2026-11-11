// Backend selection. The UI imports only this module (and ./types).

import type { Backend } from './backend';

export { BackendError, type Backend, type BackendKind } from './backend';
export * from './types';
export { base64ToBytes } from './codec';

/**
 * Pick the transport for the current environment: Tauri IPC inside the
 * desktop shell, otherwise the in-browser mock. Transports load lazily so a
 * browser build never evaluates Tauri code. A future `WasmBackend` slots in
 * here (e.g. selected by a build flag for frontend/web/).
 */
export async function createBackend(): Promise<Backend> {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    const { TauriBackend } = await import('./tauri');
    return new TauriBackend();
  }
  const { MockBackend } = await import('./mock');
  return new MockBackend();
}
