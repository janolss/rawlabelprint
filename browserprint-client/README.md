# BrowserPrint drop-in client

API-compatible TypeScript replacement for Zebra `BrowserPrint-3.1.250.min.js` **and** `BrowserPrint-Zebra-1.1.250.min.js`. One file (`BrowserPrint.min.js`) provides both globals, `BrowserPrint` and `Zebra`.

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

## Zebra helper layer (`Zebra.Printer`, `Zebra.watch`)

```js
BrowserPrint.getDefaultDevice("printer", function (device) {
  const printer = new Zebra.Printer(device);
  printer.getStatus().then((s) => console.log(s.getMessage())); // or getStatus(ok, err)
  printer.getSGD("device.host_status", console.log);
  Zebra.watch(printer, (prev, cur) => console.log(cur.getMessage()));
});
```

Supported: `getStatus`, `isPrinterReady`, `getInfo`, `getConfiguration`, `getSGD`, `setSGD`, `setThenGetSGD`, `query`, `clearRequestQueue`, `printImageAsLabel`, `getConvertedResource`, `storeConvertedResource`, `Zebra.watch`, `Zebra.stopWatching`. Every method takes `(…, success, error)` callbacks, or returns a Promise when none are given, like Zebra's.

### Provenance

`src/zebra.ts` is an **independent implementation**. It is functionally compatible with Zebra's public API (names, argument order, callback/Promise duality, result shapes) but **contains no Zebra source code**; it is written from the observable API behaviour and Zebra's published command formats (`~HS`, `~HI`, `^HH`, SGD `getvar`/`setvar`). Zebra and Browser Print are trademarks of Zebra Technologies.

### Differences from Zebra (compatible)

- `Zebra.Printer.Status/Info/Configuration` exist from the start, not after the first `Printer` is constructed.
- `~HS` is parsed per field instead of fixed character offsets (works with `\n` or `\r\n`); extra fields such as `bufferFull`, `corruptRam`, `labelsRemaining` are added.
- `Configuration` tolerates missing keys (`NaN`/empty) instead of failing.
- A failed request no longer causes the next queued request to be skipped; `setSGD` is queued so it cannot interleave with a pending query.
- `isPrinterReady` forwards transport errors to the error callback.
- The background `^HH` load stops after 5 attempts (exponential back-off) and can be disabled: `new Zebra.Printer(device, { autoLoadConfiguration: false })`.
- `Zebra.watch` polls only while something is watched, never overlaps polls per printer, and accepts a plain `Device`.
- Conversion helpers copy your `options` instead of mutating them.
- Errors in callbacks and rejections are strings, as in Zebra.

### Limits

`printImageAsLabel`, `getConvertedResource` and `storeConvertedResource` call `/convert`. RawLabelPrint only passes through raw ZPL/EPL, so image/PDF input fails with the agent's error message (see `docs/browserprint-compat-gap.md`).

## v1 scope

- Mirrors `BrowserPrint-3.1.250` and `BrowserPrint-Zebra-1.1.250`
- `fetch` + timeouts + string error callbacks
- Origin allowlisting is handled by RawLabelPrint’s HTTP server, not this client
