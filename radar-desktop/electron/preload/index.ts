import { contextBridge, ipcRenderer } from 'electron'

// ── Exposed API surface ────────────────────────────────────────────────────────

const driftApi = {
  getApiUrl: (): Promise<string> => ipcRenderer.invoke('get-api-url'),
}

contextBridge.exposeInMainWorld('drift', driftApi)

// ── TypeScript augmentation ────────────────────────────────────────────────────
// This runs in the preload context but the declaration is picked up by
// tsconfig.web.json via the include glob so the renderer gets proper types.

export type DriftBridge = typeof driftApi

declare global {
  interface Window {
    drift: DriftBridge
  }
}
