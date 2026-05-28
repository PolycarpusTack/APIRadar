import { app, BrowserWindow, ipcMain } from 'electron'
import { join } from 'path'
import { spawn, spawnSync } from 'child_process'
import type { ChildProcess } from 'child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync, unlinkSync, createWriteStream, renameSync } from 'fs'
import type { WriteStream } from 'fs'
import { tmpdir } from 'os'

// ── API sidecar ────────────────────────────────────────────────────────────────

let apiProcess: ChildProcess | null = null

function userDataDir(): string {
  return app.getPath('userData')
}

function sidecarLogPath(): string {
  return join(userDataDir(), 'radar-api.log')
}

function sidecarDbPath(): string {
  return join(userDataDir(), 'drift.db')
}

// ── PID file — stale-sidecar cleanup ──────────────────────────────────────────
// On crash or force-kill, before-quit never fires and the sidecar survives.
// We write its PID on spawn and kill it on next startup so port 17380 is free.

function pidFilePath(): string {
  return join(userDataDir(), 'radar-api.pid')
}

function killStaleSidecar(): void {
  const pidFile = pidFilePath()
  if (!existsSync(pidFile)) return
  try {
    const pid = parseInt(readFileSync(pidFile, 'utf8').trim(), 10)
    if (Number.isFinite(pid) && pid > 0) {
      if (process.platform === 'win32') {
        // taskkill /F forces immediate termination; /T kills the process tree.
        spawnSync('taskkill', ['/F', '/T', '/PID', String(pid)], { stdio: 'ignore' })
      } else {
        try { process.kill(pid) } catch { /* already dead */ }
      }
      console.log(`[main] Killed stale radar-api sidecar (pid ${pid})`)
    }
  } catch {
    // Corrupt PID file or process already gone — proceed normally.
  }
  try { unlinkSync(pidFile) } catch { /* best-effort */ }
}

function writePidFile(pid: number): void {
  try {
    writeFileSync(pidFilePath(), String(pid), 'utf8')
  } catch (err) {
    console.warn('[main] Failed to write PID file:', err)
  }
}

function deletePidFile(): void {
  try { unlinkSync(pidFilePath()) } catch { /* best-effort */ }
}

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
  // Electron does not guarantee userData exists before first write — create it
  // now so SQLite can create drift.db inside it.
  const dataDir = userDataDir()
  mkdirSync(dataDir, { recursive: true })

  const logPath = sidecarLogPath()
  const logStream = createWriteStream(logPath, { flags: 'w' })
  const logLine = (s: string) => { console.log(s); logStream.write(s + '\n') }

  logLine(`[main] radar-api sidecar starting — ${new Date().toISOString()}`)
  logLine(`[main] userData: ${dataDir}`)

  const dbPath = `sqlite:${sidecarDbPath()}`
  // Bind to loopback only — sidecar must never be reachable from the network in desktop mode.
  const args = ['--db', dbPath, '--bind', '127.0.0.1:17380']

  const bin = resolveApiBinary()
  logLine(`[main] resolved binary: ${bin ?? '(not found)'}`)

  if (!bin) {
    // Development fallback: attempt `cargo run` from the workspace root
    const workspaceRoot = join(__dirname, '..', '..', '..')
    logLine(
      '[main] radar-api binary not found — falling back to `cargo run`. ' +
        'Run `cargo build -p radar-api` to avoid this slow start.'
    )
    try {
      const cargoProcess = spawn('cargo', ['run', '--bin', 'radar-api', '--', ...args], {
        cwd: workspaceRoot,
        stdio: ['ignore', 'pipe', 'pipe'],
        detached: false,
      })
      wireProcessLogs(cargoProcess, 'radar-api(cargo)', logStream)
      if (cargoProcess.pid != null) { writePidFile(cargoProcess.pid); logLine(`[main] pid: ${cargoProcess.pid}`) }
      return cargoProcess
    } catch (err) {
      const msg = `[main] Failed to start radar-api via cargo run: ${String(err)}`
      logLine(msg)
      logStream.end()
      return null
    }
  }

  try {
    const proc = spawn(bin, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      detached: false,
    })
    wireProcessLogs(proc, 'radar-api', logStream)
    if (proc.pid != null) { writePidFile(proc.pid); logLine(`[main] pid: ${proc.pid}`) }
    return proc
  } catch (err) {
    const msg = `[main] Failed to spawn radar-api binary: ${String(err)}`
    logLine(msg)
    logStream.end()
    return null
  }
}

function wireProcessLogs(proc: ChildProcess, label: string, logStream?: WriteStream): void {
  const write = (line: string) => {
    if (logStream && !logStream.closed) logStream.write(line + '\n')
  }
  proc.stdout?.on('data', (chunk: Buffer) => {
    for (const line of chunk.toString().split('\n')) {
      if (line.trim()) { console.log(`[${label}] ${line}`); write(line) }
    }
  })
  proc.stderr?.on('data', (chunk: Buffer) => {
    for (const line of chunk.toString().split('\n')) {
      if (line.trim()) { console.error(`[${label}] ${line}`); write(line) }
    }
  })
  proc.on('exit', (code, signal) => {
    const msg = `[${label}] exited code=${String(code)} signal=${String(signal)}`
    console.log(msg)
    write(msg)
    logStream?.end()
  })
  proc.on('error', (err) => {
    const msg = `[${label}] process error: ${String(err)}`
    console.error(msg)
    write(msg)
    logStream?.end()
  })
}

// ── Health poll ────────────────────────────────────────────────────────────────

function waitForApi(url: string, maxRetries: number, proc: ChildProcess | null): Promise<void> {
  return new Promise((resolve, reject) => {
    let attempts = 0
    let settled = false

    const cleanup = () => {
      proc?.off('exit', onExit)
      proc?.off('error', onError)
    }

    const fail = (err: Error) => {
      if (settled) return
      settled = true
      cleanup()
      reject(err)
    }

    const pass = () => {
      if (settled) return
      settled = true
      cleanup()
      resolve()
    }

    const onExit = (code: number | null, signal: NodeJS.Signals | null) => {
      fail(new Error(`radar-api exited before health check passed (code=${String(code)} signal=${String(signal)})`))
    }

    const onError = (err: Error) => {
      fail(err)
    }

    proc?.once('exit', onExit)
    proc?.once('error', onError)

    function attempt() {
      if (settled) return
      attempts++
      fetch(url)
        .then((res) => {
          if (res.ok) {
            console.log(`[main] radar-api healthy after ${attempts} attempt(s)`)
            pass()
          } else {
            retry()
          }
        })
        .catch(() => retry())
    }

    function retry() {
      if (settled) return
      if (attempts >= maxRetries) {
        fail(new Error(`radar-api did not become healthy after ${maxRetries} attempts`))
        return
      }
      setTimeout(attempt, 500)
    }

    attempt()
  })
}

function sidecarLogContainsMigrationChecksumError(): boolean {
  try {
    const log = readFileSync(sidecarLogPath(), 'utf8')
    return log.includes('previously applied but has been modified')
  } catch {
    return false
  }
}

function backupIncompatibleSqliteDb(): boolean {
  const db = sidecarDbPath()
  if (!existsSync(db)) return false

  const stamp = new Date().toISOString().replace(/[:.]/g, '-')
  const backup = `${db}.migration-backup-${stamp}`

  try {
    renameSync(db, backup)
    for (const suffix of ['-wal', '-shm']) {
      const sidecarFile = `${db}${suffix}`
      if (existsSync(sidecarFile)) {
        renameSync(sidecarFile, `${backup}${suffix}`)
      }
    }
    console.warn(`[main] Backed up incompatible SQLite DB to ${backup}`)
    return true
  } catch (err) {
    console.error('[main] Failed to back up incompatible SQLite DB:', err)
    return false
  }
}

function setSplashStatus(splash: BrowserWindow, message: string): void {
  if (splash.isDestroyed()) return
  const escaped = JSON.stringify(message)
  void splash.webContents.executeJavaScript(
    `document.getElementById('status') && (document.getElementById('status').textContent = ${escaped})`,
  )
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
      animation:fill 28s cubic-bezier(.1,0,.3,1) forwards;
    }
    @keyframes fill{0%{width:0}10%{width:25%}30%{width:50%}60%{width:72%}85%{width:85%}100%{width:92%}}
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
    <span class="status" id="status">Starting services…</span>
  </div>
  <div class="track"><div class="bar"></div></div>
  <script>
    var msgs = [
      [8000,  'Starting — this may take a moment on first run…'],
      [15000, 'Still starting — security scan may be in progress…'],
      [22000, 'Almost there…']
    ];
    msgs.forEach(function(m) {
      setTimeout(function() {
        var el = document.getElementById('status');
        if (el) el.textContent = m[1];
      }, m[0]);
    });
  </script>
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

  // Packaged:    process.resourcesPath/icon.ico  (copied via extraResources in build.config.json)
  // Development: radar-desktop/resources/icon.ico (the fixed multi-size ICO built by Pillow)
  const iconPath = app.isPackaged
    ? join(process.resourcesPath ?? '', 'icon.ico')
    : join(__dirname, '..', '..', 'resources', 'icon.ico')

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
    // Production: load built renderer.
    // __dirname here is <asar>/out/main — one level up is <asar>/out,
    // so renderer/index.html is at ../renderer/index.html (not ../../).
    void win.loadFile(join(__dirname, '../renderer/index.html'))
  }

  return win
}

// ── IPC handlers ───────────────────────────────────────────────────────────────

ipcMain.handle('get-api-url', () => 'http://127.0.0.1:17380')

// ── App lifecycle ──────────────────────────────────────────────────────────────

// Tell Windows to group this app under our appId rather than the generic
// electron.exe AppUserModelId — required for the taskbar icon to reflect
// the BrowserWindow icon instead of the Electron default.
if (process.platform === 'win32') {
  app.setAppUserModelId('com.radarmonitor.desktop')
}

app.whenReady().then(async () => {
  const splash = createSplashWindow()

  killStaleSidecar()
  apiProcess = startApiSidecar()

  try {
    await waitForApi('http://127.0.0.1:17380/health', 60, apiProcess)
  } catch (err) {
    console.warn('[main] radar-api health check failed:', err)

    if (sidecarLogContainsMigrationChecksumError() && backupIncompatibleSqliteDb()) {
      setSplashStatus(splash, 'Updating local database...')
      apiProcess = startApiSidecar()
      try {
        await waitForApi('http://127.0.0.1:17380/health', 60, apiProcess)
      } catch (retryErr) {
        console.error('[main] radar-api retry after database backup failed:', retryErr)
        setSplashStatus(splash, 'API failed to start. See radar-api.log in app data.')
        return
      }
    } else {
      setSplashStatus(splash, 'API failed to start. See radar-api.log in app data.')
      return
    }
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
  deletePidFile()
})
