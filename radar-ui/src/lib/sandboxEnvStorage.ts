/**
 * localStorage fallback for Playground sandbox environments (offline / no server).
 *
 * Environments are normally stored server-side (POST /v1/sandbox-envs) precisely
 * so bearer tokens never sit in the browser. When the server is unreachable the
 * page falls back to this module.
 *
 * Security invariant: bearer tokens are NEVER persisted here. `saveLocalEnvs`
 * strips `bearer_token` before writing (tokens live only in React state for the
 * current tab), and `loadLocalEnvs` purges any legacy plaintext token an older
 * build may have written, rewriting the cleaned list.
 */

export const SANDBOX_ENVS_LOCAL_KEY = 'drift-playground-envs-local'

export interface SandboxEnv {
  id: string
  name: string
  base_url: string
  bearer_token: string
  description: string
  created_at?: string
  updated_at?: string
}

function stripToken(env: SandboxEnv): SandboxEnv {
  return { ...env, bearer_token: '' }
}

export function loadLocalEnvs(): SandboxEnv[] {
  try {
    const raw = localStorage.getItem(SANDBOX_ENVS_LOCAL_KEY)
    if (!raw) return []
    const envs = JSON.parse(raw) as SandboxEnv[]
    // Purge legacy plaintext tokens written by older builds.
    if (envs.some((e) => e.bearer_token)) {
      const cleaned = envs.map(stripToken)
      saveLocalEnvs(cleaned)
      return cleaned
    }
    return envs
  } catch {
    return []
  }
}

export function saveLocalEnvs(envs: SandboxEnv[]) {
  try {
    localStorage.setItem(SANDBOX_ENVS_LOCAL_KEY, JSON.stringify(envs.map(stripToken)))
  } catch {}
}
