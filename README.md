# RawLabelPrint

macOS (Apple Silicon) tray app that replaces [Zebra Browser Print](https://www.zebra.com/us/en/products/software/barcode-printers/link-os/browser-print.html) for browser-based raw label printing.

The app:

- Lives in the menu bar (tray)
- Exposes a localhost HTTP API (default `http://127.0.0.1:9100`)
- Discovers Zebra printers on the LAN via UDP broadcast on port **4201** (Browser Print protocol)
- Sends raw data (**ZPL/EPL/…**) **directly over TCP** to the printer print port (default **9100**) — not via CUPS
- Optional **Compatible mode**: same HTTP surface as Zebra Browser Print so existing `BrowserPrint.js` pages work as a drop-in in mixed environments

## Quick start (simple API)

List printers (GET):

```
http://127.0.0.1:9100/
```

Response shape:

```json
[{ "Name": "ZD421 (192.168.1.50)" }]
```

Print (GET):

```
http://127.0.0.1:9100/?printer=ZD421%20(192.168.1.50)&data=^XA...^XZ
```

Print (POST):

```json
{
  "printer": "ZD421 (192.168.1.50)",
  "data": "^XA^FO50,50^A0N,40,40^FDHello^FS^XZ"
}
```

If `printer` is omitted/empty, the **default printer** selected in Settings is used.

CORS: `Access-Control-Allow-Origin: *`

Open `test/index.html` in a browser for a simple interactive test page.

## Compatible mode (Zebra Browser Print API)

Enabled by default in Settings (“Compatible mode”). When on, the same endpoints that `BrowserPrint-3.x.js` calls are available:

| Method | Path | Purpose |
|---|---|---|
| GET/POST | `/available` | List devices as `{ "printer": [ Device, … ], "deviceList": […] }` |
| GET/POST | `/default` / `/default?type=printer` | Default device JSON, or empty body if none |
| GET | `/config` | Application configuration stub |
| POST | `/write` | JSON (`device.send` / `sendUrl`) **or** multipart `json`+`blob` (`device.sendFile`) → TCP RAW write |
| POST | `/read` | `{ "device": { "uid", … } }` → short read on same TCP session |
| POST | `/convert` | BrowserPrint convert / `convertAndSendFile` (raw ZPL passthrough; image/PDF conversion not supported) |
| POST | `/convert/scan` | BrowserPrint `scanImage` stub (same limits as `/convert`) |

Device `uid` is the printer serial when known, otherwise `net:<ip>:<printPort>`.

`/write` accepts both BrowserPrint body styles:

- **JSON** (from `device.send`): `{ "device": { "uid", … }, "data": "^XA…" }`
- **multipart/form-data** (from `device.sendFile`): field `json` = `{ "device": { … } }`, field `blob` = raw bytes (ZPL/PDF/…)

Web apps that already use Zebra’s JS library (`BrowserPrint.getDefaultDevice` / `getLocalDevices` / `device.send` / `device.sendFile`) can keep pointing at `http://127.0.0.1:9100/` without code changes — use RawLabelPrint on Apple Silicon machines and official Browser Print elsewhere.

Disable Compatible mode in Settings if you only want the simple `/` API.

## Configure (Settings)

1. Launch **RawLabelPrint** (menu bar icon appears).
2. Click the tray icon (or **Settings…**).
3. Click **Search Zebra printers**, wait ~5 seconds, then **Set as default**.
4. Optionally add a printer manually (name, IP, print port).
5. Adjust HTTP port if needed and **Save & apply** (bind address is fixed to `127.0.0.1`).
6. Keep **Compatible mode** on for BrowserPrint.js clients.
7. Optional: enable **Launch at login**.

### macOS Local Network

On macOS 15+, allow **RawLabelPrint** under **System Settings → Privacy & Security → Local Network**.  
The app browses Bonjour (`_printer._tcp`) at startup to trigger that prompt (menu bar apps otherwise often never show it).  

UDP discovery can succeed while TCP `:9100` still fails with `No route to host (os error 65)` until Local Network is fully granted for the same `.app` you actually run (prefer one install: `/Applications/RawLabelPrint.app`, not a `target/release/...` copy).

Config is stored at:

`~/Library/Application Support/se.rawlabelprint.app/config.json`

## Develop (on a Mac)

Prerequisites: Node.js 20+, Rust (stable), Xcode command line tools.

```bash
npm install
npm run tauri dev
```

## Build DMG for Apple Silicon

On an Apple Silicon Mac:

```bash
npm install
npm run tauri build
```

Artifacts:

- `src-tauri/target/release/bundle/macos/RawLabelPrint.app`
- `src-tauri/target/release/bundle/dmg/RawLabelPrint_*.dmg`

### Signing / notarization (recommended for distribution)

1. Apple Developer ID Application certificate
2. Configure Tauri signing env vars (`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, …) — see [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/)
3. Rebuild; Gatekeeper will accept the notarized DMG

Without notarization: distribute the DMG and instruct users to **right-click → Open** the first time (or allow under System Settings → Privacy & Security). macOS may also ask for **Local Network** permission (required for UDP discovery / TCP print).

## Project layout

| Path | Role |
|---|---|
| `ui/` | Settings UI (Vite + vanilla JS) |
| `src-tauri/` | Rust: tray, HTTP API, UDP discovery, TCP print |
| `test/` | Browser API smoke-test page |
| `reference/` | Reference implementations (local only, gitignored) |

## Notes

- Do not run Zebra Browser Print at the same time (both use UDP **4201** / HTTP **9100**).
- Printers must be reachable on the LAN (Wi‑Fi/Ethernet). USB-only printers are out of scope for this version.
- HTTPS on port **9101** (Safari + https pages) is not implemented yet; use HTTP pages or Chrome against port **9100**.
