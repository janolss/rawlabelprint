# BrowserPrint drop-in client

API-compatible TypeScript replacement for Zebra `BrowserPrint-3.1.250.min.js`.

Talks to a localhost Browser Print agent (RawLabelPrint Compatible mode or official Zebra Browser Print) at `http://127.0.0.1:9100/`.

## Drop-in usage

```html
<script src="/path/to/BrowserPrint.min.js"></script>
<script>
  BrowserPrint.getDefaultDevice(
    "printer",
    function (device) {
      if (!device) return;
      device.send("^XA^FO50,50^A0N,40,40^FDHello^FS^XZ");
    },
    function (err) {
      console.error(err);
    }
  );
</script>
```

Built artifact: [`dist/BrowserPrint.min.js`](dist/BrowserPrint.min.js) (IIFE, global `BrowserPrint`).

ESM: `import BrowserPrint from '@rawlabelprint/browserprint-client'`.

## Build / test

From repo root:

```bash
npm run build:browserprint
npm run test:browserprint
```

Or inside this folder:

```bash
npm install
npm test
```

## v1 scope

- Mirrors base `BrowserPrint-3.1.250` (not `BrowserPrint-Zebra-1.1.250`)
- `fetch` + timeouts + string error callbacks
- Origin allowlisting is handled by RawLabelPrint’s HTTP server, not this client
