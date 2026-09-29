# RawLabelPrint

Raw Label Print allows sending raw data directly to local printers through http://localhost:9100 (configurable). The intended use-case is label printing from a browser and the application is createad because i needed to replace [Zebra Browser Print](https://www.zebra.com/us/en/products/software/barcode-printers/link-os/browser-print.html) on Apple Silicon.

## Quick Start Instructions

Get list of available printers (GET):
```
http://localhost:9100
```

Send print data to printer (GET):
```
http://localhost:9100/?printer=printer1&data=rawdata
```

Send data to printer (POST):
```
endpoint: http://localhost:9100/

body:
{
  "printer": "printer1",
  "data": "rawlabeldata"
}
```
