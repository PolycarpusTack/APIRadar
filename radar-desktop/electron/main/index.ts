import { app, BrowserWindow, ipcMain } from 'electron'
import { join } from 'path'
import { spawn } from 'child_process'
import type { ChildProcess } from 'child_process'
import { existsSync } from 'fs'

// ── API sidecar ────────────────────────────────────────────────────────────────

let apiProcess: ChildProcess | null = null

function resolveApiBinary(): string | null {
  // 1. Explicit override for development / CI
  const envBin = process.env['RADAR_API_BIN']
  if (envBin && existsSync(envBin)) {
    return envBin
  }

  // 2. Production: binary lives next to the app resources
  const resourcesBin = join(process.resourcesPath ?? '', 'radar-api')
  if (existsSync(resourcesBin)) {
    return resourcesBin
  }
  const resourcesBinExe = join(process.resourcesPath ?? '', 'radar-api.exe')
  if (existsSync(resourcesBinExe)) {
    return resourcesBinExe
  }

  return null
}

function startApiSidecar(): ChildProcess | null {
  const dbPath = `sqlite:${join(app.getPath('userData'), 'drift.db')}`
  // Bind to loopback only — sidecar must never be reachable from the network in desktop mode.
  const args = ['--db', dbPath, '--bind', '127.0.0.1:8080']

  const bin = resolveApiBinary()

  if (!bin) {
    // Development fallback: attempt `cargo run` from the workspace root
    const workspaceRoot = join(__dirname, '..', '..', '..', '..')
    console.warn(
      '[main] radar-api binary not found — falling back to `cargo run`. ' +
        'Run `cargo build -p radar-api` to avoid this slow start.'
    )
    try {
      const cargoProcess = spawn('cargo', ['run', '--bin', 'radar-api', '--', ...args], {
        cwd: workspaceRoot,
        stdio: ['ignore', 'pipe', 'pipe'],
        detached: false,
      })
      wireProcessLogs(cargoProcess, 'radar-api(cargo)')
      return cargoProcess
    } catch (err) {
      console.error('[main] Failed to start radar-api via cargo run:', err)
      return null
    }
  }

  try {
    const proc = spawn(bin, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      detached: false,
    })
    wireProcessLogs(proc, 'radar-api')
    return proc
  } catch (err) {
    console.error('[main] Failed to spawn radar-api binary:', err)
    return null
  }
}

function wireProcessLogs(proc: ChildProcess, label: string): void {
  proc.stdout?.on('data', (chunk: Buffer) => {
    for (const line of chunk.toString().split('\n')) {
      if (line.trim()) console.log(`[${label}] ${line}`)
    }
  })
  proc.stderr?.on('data', (chunk: Buffer) => {
    for (const line of chunk.toString().split('\n')) {
      if (line.trim()) console.error(`[${label}] ${line}`)
    }
  })
  proc.on('exit', (code, signal) => {
    console.log(`[${label}] exited code=${String(code)} signal=${String(signal)}`)
  })
  proc.on('error', (err) => {
    console.error(`[${label}] process error:`, err)
  })
}

// ── Health poll ────────────────────────────────────────────────────────────────

function waitForApi(url: string, maxRetries: number): Promise<void> {
  return new Promise((resolve, reject) => {
    let attempts = 0

    function attempt() {
      attempts++
      fetch(url)
        .then((res) => {
          if (res.ok) {
            console.log(`[main] radar-api healthy after ${attempts} attempt(s)`)
            resolve()
          } else {
            retry()
          }
        })
        .catch(() => retry())
    }

    function retry() {
      if (attempts >= maxRetries) {
        reject(new Error(`radar-api did not become healthy after ${maxRetries} attempts`))
        return
      }
      setTimeout(attempt, 500)
    }

    attempt()
  })
}

// ── Window factory ─────────────────────────────────────────────────────────────

function createWindow(): BrowserWindow {
  const preloadPath = join(__dirname, '../preload/index.js')

  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    backgroundColor: '#0B0F19', // matches --bg-base token; Electron reads this before CSS loads
    titleBarStyle: 'default',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      preload: preloadPath,
    },
  })

  if (!app.isPackaged) {
    // Development: load Vite dev server
    void win.loadURL('http://localhost:5173')
    win.webContents.openDevTools({ mode: 'detach' })
  } else {
    // Production: load built renderer
    void win.loadFile(join(__dirname, '../../renderer/index.html'))
  }

  return win
}

// ── IPC handlers ───────────────────────────────────────────────────────────────

ipcMain.handle('get-api-url', () => 'http://127.0.0.1:8080')

// ── App lifecycle ──────────────────────────────────────────────────────────────

app.whenReady().then(async () => {
  apiProcess = startApiSidecar()

  try {
    await waitForApi('http://127.0.0.1:8080/health', 20)
  } catch (err) {
    console.warn('[main] radar-api health check failed — continuing anyway:', err)
  }

  createWindow()

  app.on('activate', () => {
    // macOS: re-create window when dock icon is clicked with no open windows
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on('window-all-closed', () => {
  // Quit on all platforms (override macOS default keep-alive behaviour)
  app.quit()
})

app.on('before-quit', () => {
  if (apiProcess && !apiProcess.killed) {
    console.log('[main] Stopping radar-api sidecar…')
    apiProcess.kill('SIGTERM')
    apiProcess = null
  }
})
