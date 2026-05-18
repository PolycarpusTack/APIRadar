"use strict";
const electron = require("electron");
const path = require("path");
const child_process = require("child_process");
const fs = require("fs");
let apiProcess = null;
function resolveApiBinary() {
  const envBin = process.env["DRIFT_API_BIN"];
  if (envBin && fs.existsSync(envBin)) {
    return envBin;
  }
  const resourcesBin = path.join(process.resourcesPath ?? "", "drift-api");
  if (fs.existsSync(resourcesBin)) {
    return resourcesBin;
  }
  const resourcesBinExe = path.join(process.resourcesPath ?? "", "drift-api.exe");
  if (fs.existsSync(resourcesBinExe)) {
    return resourcesBinExe;
  }
  return null;
}
function startApiSidecar() {
  const dbPath = `sqlite:${path.join(electron.app.getPath("userData"), "drift.db")}`;
  const args = ["--db", dbPath];
  const bin = resolveApiBinary();
  if (!bin) {
    const workspaceRoot = path.join(__dirname, "..", "..", "..", "..");
    console.warn(
      "[main] drift-api binary not found — falling back to `cargo run`. Run `cargo build -p drift-api` to avoid this slow start."
    );
    try {
      const cargoProcess = child_process.spawn("cargo", ["run", "--bin", "drift-api", "--", ...args], {
        cwd: workspaceRoot,
        stdio: ["ignore", "pipe", "pipe"],
        detached: false
      });
      wireProcessLogs(cargoProcess, "drift-api(cargo)");
      return cargoProcess;
    } catch (err) {
      console.error("[main] Failed to start drift-api via cargo run:", err);
      return null;
    }
  }
  try {
    const proc = child_process.spawn(bin, args, {
      stdio: ["ignore", "pipe", "pipe"],
      detached: false
    });
    wireProcessLogs(proc, "drift-api");
    return proc;
  } catch (err) {
    console.error("[main] Failed to spawn drift-api binary:", err);
    return null;
  }
}
function wireProcessLogs(proc, label) {
  proc.stdout?.on("data", (chunk) => {
    for (const line of chunk.toString().split("\n")) {
      if (line.trim()) console.log(`[${label}] ${line}`);
    }
  });
  proc.stderr?.on("data", (chunk) => {
    for (const line of chunk.toString().split("\n")) {
      if (line.trim()) console.error(`[${label}] ${line}`);
    }
  });
  proc.on("exit", (code, signal) => {
    console.log(`[${label}] exited code=${String(code)} signal=${String(signal)}`);
  });
  proc.on("error", (err) => {
    console.error(`[${label}] process error:`, err);
  });
}
function waitForApi(url, maxRetries) {
  return new Promise((resolve, reject) => {
    let attempts = 0;
    function attempt() {
      attempts++;
      fetch(url).then((res) => {
        if (res.ok) {
          console.log(`[main] drift-api healthy after ${attempts} attempt(s)`);
          resolve();
        } else {
          retry();
        }
      }).catch(() => retry());
    }
    function retry() {
      if (attempts >= maxRetries) {
        reject(new Error(`drift-api did not become healthy after ${maxRetries} attempts`));
        return;
      }
      setTimeout(attempt, 500);
    }
    attempt();
  });
}
function createWindow() {
  const preloadPath = path.join(__dirname, "../preload/index.js");
  const win = new electron.BrowserWindow({
    width: 1280,
    height: 800,
    backgroundColor: "#0f0f14",
    titleBarStyle: "default",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      preload: preloadPath
    }
  });
  if (!electron.app.isPackaged) {
    void win.loadURL("http://localhost:5173");
    win.webContents.openDevTools({ mode: "detach" });
  } else {
    void win.loadFile(path.join(__dirname, "../../renderer/index.html"));
  }
  return win;
}
electron.ipcMain.handle("get-api-url", () => "http://127.0.0.1:8080");
electron.app.whenReady().then(async () => {
  apiProcess = startApiSidecar();
  try {
    await waitForApi("http://127.0.0.1:8080/health", 20);
  } catch (err) {
    console.warn("[main] drift-api health check failed — continuing anyway:", err);
  }
  createWindow();
  electron.app.on("activate", () => {
    if (electron.BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});
electron.app.on("window-all-closed", () => {
  electron.app.quit();
});
electron.app.on("before-quit", () => {
  if (apiProcess && !apiProcess.killed) {
    console.log("[main] Stopping drift-api sidecar…");
    apiProcess.kill("SIGTERM");
    apiProcess = null;
  }
});
