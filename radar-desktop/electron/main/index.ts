import { app, BrowserWindow, ipcMain } from 'electron'
import { join } from 'path'
import { spawn } from 'child_process'
import type { ChildProcess } from 'child_process'
import { existsSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'

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

  // 3. Development: workspace release build (cargo build -p radar-api --release)
  if (!app.isPackaged) {
    const devBin = join(__dirname, '..', '..', '..', 'target', 'release', 'radar-api')
    if (existsSync(devBin)) return devBin
    const devBinExe = join(__dirname, '..', '..', '..', 'target', 'release', 'radar-api.exe')
    if (existsSync(devBinExe)) return devBinExe
  }

  return null
}

function startApiSidecar(): ChildProcess | null {
  const dbPath = `sqlite:${join(app.getPath('userData'), 'drift.db')}`
  // Bind to loopback only — sidecar must never be reachable from the network in desktop mode.
  const args = ['--db', dbPath, '--bind', '127.0.0.1:17380']

  const bin = resolveApiBinary()

  if (!bin) {
    // Development fallback: attempt `cargo run` from the workspace root
    const workspaceRoot = join(__dirname, '..', '..', '..')
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

// ── Splash screen ──────────────────────────────────────────────────────────────

function getSplashHtml(): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8"/>
  <style>
    *{margin:0;padding:0;box-sizing:border-box}
    html,body{
      width:100%;height:100%;
      background:#0B0F19;
      display:flex;flex-direction:column;
      align-items:center;justify-content:center;
      overflow:hidden;user-select:none;-webkit-user-select:none;
      font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
      -webkit-font-smoothing:antialiased;
    }
    .content{
      display:flex;flex-direction:column;align-items:center;gap:20px;
      animation:appear .5s cubic-bezier(.4,0,.2,1) both;
    }
    @keyframes appear{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:translateY(0)}}
    svg.icon{
      width:96px;height:96px;
      filter:drop-shadow(0 0 24px rgba(56,5,227,.45));
    }
    .ping-outer{animation:ping 2.5s ease-in-out infinite}
    @keyframes ping{0%,100%{fill-opacity:.18}50%{fill-opacity:.5}}
    .wordmark{display:flex;flex-direction:column;align-items:center;gap:1px}
    .wm-api{
      font-family:'Courier New',Courier,monospace;
      font-size:9.5px;font-weight:600;letter-spacing:3px;
      color:#5B33F0;text-transform:uppercase;line-height:1.4;
    }
    .wm-radar{
      font-family:-apple-system,BlinkMacSystemFont,'Helvetica Neue',Arial,sans-serif;
      font-size:30px;font-weight:800;letter-spacing:-1.5px;
      color:#F0F4FF;line-height:1;
    }
    .wm-sub{
      font-family:'Courier New',Courier,monospace;
      font-size:9px;letter-spacing:1px;color:#3D4F68;
      text-transform:uppercase;margin-top:4px;
    }
    .status{
      font-family:'Courier New',Courier,monospace;
      font-size:10.5px;color:#3D4F68;letter-spacing:.3px;
    }
    .track{position:fixed;bottom:0;left:0;right:0;height:2px;background:#111827}
    .bar{
      height:100%;border-radius:0 1px 1px 0;
      background:linear-gradient(90deg,#3805E3 0%,#5B33F0 60%,#B3FC4F 100%);
      animation:fill 8s cubic-bezier(.1,0,.3,1) forwards;
    }
    @keyframes fill{0%{width:0}20%{width:35%}50%{width:60%}80%{width:80%}100%{width:90%}}
  </style>
</head>
<body>
  <div class="content">
    <svg class="icon" viewBox="0 0 120 120" xmlns="http://www.w3.org/2000/svg" fill="none">
      <defs>
        <linearGradient id="bg" x1="0" y1="120" x2="120" y2="0" gradientUnits="userSpaceOnUse">
          <stop stop-color="#1A0A6B"/>
          <stop offset="1" stop-color="#4515F0"/>
        </linearGradient>
      </defs>
      <rect width="120" height="120" rx="26" fill="url(#bg)"/>
      <line x1="26" y1="60" x2="94" y2="60" stroke="white" stroke-opacity=".05" stroke-width=".75"/>
      <line x1="60" y1="26" x2="60" y2="94" stroke="white" stroke-opacity=".05" stroke-width=".75"/>
      <circle cx="60" cy="60" r="20" stroke="white" stroke-opacity=".14" stroke-width=".75" stroke-dasharray="2.5 5"/>
      <circle cx="60" cy="60" r="35" stroke="white" stroke-opacity=".09" stroke-width=".75" stroke-dasharray="2.5 5"/>
      <circle cx="60" cy="60" r="1.8" fill="white" fill-opacity=".4"/>
      <!-- Sweep group rotated around icon centre (60,60).
           Sector 270°->300°; tip at r=35: x=17.5, y=-30.3 -->
      <g transform="translate(60,60)">
        <path d="M 0 0 L 0 -35 A 35 35 0 0 1 17.5 -30.3 Z" fill="#5B33F0" fill-opacity=".22"/>
        <line x1="0" y1="0" x2="17.5" y2="-30.3" stroke="white" stroke-opacity=".72" stroke-width="1.2" stroke-linecap="round"/>
        <circle cx="17.5" cy="-30.3" r="7"   fill="#B3FC4F" class="ping-outer"/>
        <circle cx="17.5" cy="-30.3" r="4.5" fill="#B3FC4F" fill-opacity=".88"/>
        <circle cx="17.5" cy="-30.3" r="2.5" fill="#EEFFAA"/>
        <animateTransform attributeName="transform" attributeType="XML"
          type="rotate" from="0" to="360" dur="2.5s"
          repeatCount="indefinite" additive="sum"/>
      </g>
    </svg>
    <div class="wordmark">
      <span class="wm-api">API</span>
      <span class="wm-radar">Radar</span>
      <span class="wm-sub">Contract Monitor</span>
    </div>
    <span class="status">Starting services…</span>
  </div>
  <div class="track"><div class="bar"></div></div>
</body>
</html>`
}

function createSplashWindow(): BrowserWindow {
  const tmpFile = join(tmpdir(), 'radar-splash.html')
  writeFileSync(tmpFile, getSplashHtml(), 'utf8')

  const splash = new BrowserWindow({
    width: 480,
    height: 300,
    frame: false,
    resizable: false,
    movable: false,
    center: true,
    show: false,
    skipTaskbar: true,
    backgroundColor: '#0B0F19',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
    },
  })

  void splash.loadFile(tmpFile)
  splash.once('ready-to-show', () => splash.show())
  return splash
}

function closeSplash(splash: BrowserWindow): void {
  if (splash.isDestroyed()) return
  let opacity = 1.0
  const timer = setInterval(() => {
    opacity -= 0.15
    if (opacity <= 0 || splash.isDestroyed()) {
      clearInterval(timer)
      if (!splash.isDestroyed()) splash.close()
    } else {
      splash.setOpacity(opacity)
    }
  }, 20)
}

// ── Window factory ─────────────────────────────────────────────────────────────

function createWindow(): BrowserWindow {
  const preloadPath = join(__dirname, '../preload/index.js')

  const iconPath = app.isPackaged
    ? join(process.resourcesPath ?? '', 'icon.ico')
    : join(__dirname, '..', '..', '..', 'radar-desktop', 'resources', 'icon.ico')

  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    show: false, // shown by app.whenReady after splash fades
    icon: existsSync(iconPath) ? iconPath : undefined,
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

ipcMain.handle('get-api-url', () => 'http://127.0.0.1:17380')

// ── App lifecycle ──────────────────────────────────────────────────────────────

app.whenReady().then(async () => {
  const splash = createSplashWindow()

  apiProcess = startApiSidecar()

  try {
    await waitForApi('http://127.0.0.1:17380/health', 20)
  } catch (err) {
    console.warn('[main] radar-api health check failed — continuing anyway:', err)
  }

  const win = createWindow()

  win.once('ready-to-show', () => {
    closeSplash(splash)
    win.show()
  })

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
