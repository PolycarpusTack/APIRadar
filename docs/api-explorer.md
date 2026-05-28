# API Explorer — Developer Guide

> **Audience:** Developers integrating with Radar, and maintainers of the Radar platform itself.  
> **Last updated:** 2026-05-27

---

## Overview

The **Playground** tab in the Radar dashboard embeds an interactive API Explorer built on [Scalar API Reference](https://scalar.com/). It lets you browse every Radar API operation, send live requests against a running instance, and save Bearer tokens as reusable sandbox environments — all without leaving the application.

The Explorer runs entirely offline. Radar serves the Scalar JavaScript bundle from its own binary (`GET /scalar.js`) rather than a CDN, so it works in air-gapped environments, on a laptop with no internet, and inside the packaged Electron desktop app.

---

## Architecture

```
SettingsPage → POST /scalar/update ──┐
                                      ▼
                          <db-dir>/scalar_override.js
                          <db-dir>/scalar_override.version
                                      │
GET /scalar.js ───────────────────────┴── fallback: include_bytes!("vendor/scalar.js")
       ▲
       │  (fetched by the Playground iframe at load time)
       │
PlaygroundPage (React)
  └── <iframe sandbox="allow-scripts allow-forms allow-popups …">
        srcdoc = <html>…<script src="http://…/scalar.js">…</html>
```

**Why a null-origin iframe?**  
The Playground iframe is created with `srcdoc` and its `sandbox` attribute deliberately omits `allow-same-origin`. This produces a *null-origin* iframe whose requests are not subject to the parent page's Content Security Policy, letting it freely load the Scalar script from the local radar-api server.  Because the iframe has a null origin, the `src` attribute of the `<script>` tag must be an absolute URL — relative URLs do not work.

**URL selection at runtime (see `PlaygroundPage.tsx`):**

| Context | Scalar URL |
|---|---|
| Packaged desktop (`file://`) | `http://127.0.0.1:17380/scalar.js` (hardcoded sidecar address) |
| Desktop dev server | `http://localhost:5181/scalar.js` → proxied to `http://127.0.0.1:17380/scalar.js` |
| Web dev server | `http://localhost:6173/scalar.js` → proxied to `http://localhost:8081/scalar.js` |
| Web production | `https://<origin>/scalar.js` (same-origin, served by radar-api) |

---

## Keeping Scalar up to date

### Option 1 — In-app update (desktop / SQLite only)

Open **Settings → API Explorer (Scalar)** and click **Check for updates**. If a newer version is available on npm, an **Update to x.y.z** button appears. Clicking it:

1. Calls `POST /scalar/update`.
2. The server downloads `@scalar/api-reference@<latest>/dist/browser/standalone.js` from jsDelivr.
3. Writes two files alongside the database:
   - `scalar_override.js` — the new bundle
   - `scalar_override.version` — the version string
4. Subsequent requests to `GET /scalar.js` serve the override instead of the compiled-in bundle.
5. The Playground reloads automatically on the next tab visit.

No restart is required. The override survives app restarts.

> This flow requires outbound HTTPS access to `registry.npmjs.org` (version check) and `cdn.jsdelivr.net` (download). In fully air-gapped environments use Option 2.

### Option 2 — Build-time vendor update

Update the compiled-in bundle when cutting a new release:

```bash
# 1. Install the latest @scalar/api-reference package at the workspace root
pnpm add -w @scalar/api-reference@latest

# 2. Copy the standalone bundle into the vendor directory and log the version
pnpm vendor:scalar
# Output: "Vendored @scalar/api-reference 1.58.2"

# 3. Record the new version in the companion file
#    (keep the trailing newline — include_str! trims it, but editors expect it)
echo "1.58.2" > radar-api/vendor/scalar.version

# 4. Commit both files — they are intentionally checked in
git add radar-api/vendor/scalar.js radar-api/vendor/scalar.version
git commit -m "chore: vendor @scalar/api-reference 1.58.2"

# 5. Rebuild
cargo build -p radar-api --release
```

> `radar-api/vendor/scalar.js` is explicitly committed and should **never** be added to `.gitignore`.

---

## API reference

### `GET /scalar.js`

Serves the active Scalar standalone bundle. No authentication required.

**Response headers:**

| Header | Compiled-in bundle | Override bundle |
|---|---|---|
| `Content-Type` | `application/javascript; charset=utf-8` | `application/javascript; charset=utf-8` |
| `Cache-Control` | `public, max-age=86400, immutable` | `public, max-age=3600` |

The compiled-in bundle is marked `immutable` (24 h) because it can only change with a binary rebuild. The override is cached for 1 hour to pick up any subsequent update quickly.

---

### `GET /scalar/version`

Returns version metadata for the Scalar bundle. No authentication required.

**Response:**

```json
{
  "bundled":          "1.57.5",
  "override":         "1.58.2",
  "active":           "1.58.2",
  "latest":           "1.58.2",
  "update_available": false
}
```

| Field | Type | Description |
|---|---|---|
| `bundled` | `string` | Version compiled into the binary |
| `override` | `string \| null` | Version of the disk override, or `null` if none exists |
| `active` | `string` | Version currently served by `GET /scalar.js` |
| `latest` | `string \| null` | Latest version on npm, or `null` if the registry was unreachable |
| `update_available` | `bool` | `true` when `latest > active` (semver comparison) |

The `latest` field is fetched live from `https://registry.npmjs.org/@scalar/api-reference/latest` on every call to this endpoint (10-second timeout). If the registry is unreachable `latest` is `null` and `update_available` is `false`.

---

### `POST /scalar/update`

Downloads the latest Scalar bundle and writes it as a disk override. No authentication required.

**Constraints:**
- Only available in **SQLite (desktop / bare-metal) mode**. Returns `400 Bad Request` for PostgreSQL deployments.
- Requires outbound internet access to `registry.npmjs.org` and `cdn.jsdelivr.net`.
- The download has a 120-second timeout.

**Response (success, HTTP 200):**

```json
{
  "updated": true,
  "version": "1.58.2",
  "bytes": 3542781
}
```

**Error responses:**

| Status | Condition |
|---|---|
| `400` | Deployment uses PostgreSQL — override not supported |
| `502` | npm registry unreachable or version could not be determined |
| `502` | CDN download returned a non-2xx status |
| `500` | Could not write override file to disk (permissions issue) |

---

## Sandbox Environments

The Playground lets you save API credentials as named **Sandbox Environments** so you do not have to paste a Bearer token on every request.

- Environments are stored in the `sandbox_env` table.
- Bearer tokens are returned masked (last 4 characters only) in all `GET` responses. The full token is stored in the database — ensure DB backups are encrypted at rest.
- To create an environment via API:

```bash
curl -X POST http://localhost:8080/v1/sandbox-envs \
  -H "Content-Type: application/json" \
  -d '{"name": "staging", "base_url": "https://api.staging.example.com", "token": "sk-..."}'
```

---

## Troubleshooting

### Playground iframe is blank

1. Confirm the sidecar is running: `GET /health` should return `{"status":"ok"}`.
2. Open DevTools → Network tab → look for a request to `/scalar.js`. If it returns 4xx or 0 bytes, the bundle is missing or the proxy is misconfigured.
3. In dev mode, confirm the Vite proxy for `/scalar` and `/scalar.js` points to the correct backend port (`8081` for web dev, `17380` for Electron dev).
4. Confirm the iframe sandbox does **not** contain `allow-same-origin` — this would reintroduce the parent CSP and block the local script.

### "Update to x.y.z" is missing after Check for updates

- `latest` was `null` — the npm registry was unreachable. Check outbound HTTPS connectivity from the machine running `radar-api`.
- The running version is already the latest — `update_available` is `false` and only the "Up to date" message is shown.

### Override is active but I want to revert to the compiled-in bundle

Delete the two override files alongside the database and they will not be served on the next request:

```powershell
# Windows (PowerShell) — adjust path to match your --db flag
$dir = "$env:APPDATA\radar-desktop"
Remove-Item "$dir\scalar_override.js"
Remove-Item "$dir\scalar_override.version"
```

```bash
# macOS / Linux
rm ~/Library/Application\ Support/radar-desktop/scalar_override.{js,version}
```

### Update fails with "Download failed: HTTP 404"

The version string from the npm registry did not match a published jsDelivr package path. This can happen if npm reports a brand-new release that jsDelivr has not replicated yet (jsDelivr has a ~2-minute propagation lag). Wait a few minutes and try again.

---

## Developer notes

### Serving mechanism (`radar-api/src/scalar_update.rs`)

| Symbol | Purpose |
|---|---|
| `OVERRIDE_DIR` | `OnceLock<Option<PathBuf>>` — set once in `run()` from the resolved SQLite path |
| `active_js()` | Returns `(Vec<u8>, bool)` — bytes and `is_bundled` flag |
| `active_version()` | String version currently being served |
| `get_version()` | Handler for `GET /scalar/version` |
| `post_update()` | Handler for `POST /scalar/update` |

`OVERRIDE_DIR` is never set during tests (`run()` is never called in the test suite), so all tests get the compiled-in bundle — no disk I/O in unit tests.

### Vendoring the bundle (`pnpm vendor:scalar`)

The root `package.json` `vendor:scalar` script copies `node_modules/@scalar/api-reference/dist/browser/standalone.js` to `radar-api/vendor/scalar.js`. Run it whenever you update the `@scalar/api-reference` npm package.

### Adding the Playground to a new deployment context

The `SCALAR_SRC` constant in `PlaygroundPage.tsx` selects the URL at module initialisation:

```typescript
const SCALAR_SRC: string = (() => {
  if (window.location.protocol === 'file:') {
    return 'http://127.0.0.1:17380/scalar.js'   // packaged desktop
  }
  return `${window.location.origin}/scalar.js`   // web (dev or prod)
})()
```

If you run radar-api on a different port or behind a prefix, update this constant or extend the condition.
