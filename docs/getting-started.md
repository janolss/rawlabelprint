<!-- Source for the tray “Getting Started…” page. Run: npm run docs:getting-started -->

# Getting Started with RawLabelPrint

RawLabelPrint is a small background app for **macOS**, **Windows**, and **Linux**. It lives in the menu bar / system tray and lets approved websites send raw label data (ZPL/EPL) straight to your saved Zebra printers.

You do not print through the normal system print dialog (CUPS). The website talks to RawLabelPrint on this computer, and RawLabelPrint talks to the printer.

---

## 1. Open Settings

1. Launch **RawLabelPrint**. An icon appears in the menu bar (macOS) or system tray (Windows / Linux).
2. Click the icon, or open the tray menu and choose **Settings…**.

Closing the Settings window hides it; the app keeps running. Use **Quit** in the tray menu to stop it.

On Windows the icon may sit in the taskbar overflow area (^). Drag it out so it stays visible.

---

## 2. Add your first printer

1. In Settings, open **Printers**.
2. Click **Search Zebra printers** and wait a few seconds (LAN and USB).
3. On the printer you want: optionally **Test connection**, check **Make default**, then **Save**.

A search result is **not** available to websites until you save it.

**Alternatives**

- **Add manually** — name, IP address, and print port (usually `9100`).
- **Virtual printer** — for testing without hardware (jobs appear under **Prints**).

Use **Test print** on a saved printer to confirm RawLabelPrint can reach it. Status shows **Online** or **Offline**; use **↻** to recheck.

---

## 3. Approve websites that print

The first time a website lists printers or sends a print job, RawLabelPrint blocks it until you allow that site.

1. Try printing (or listing printers) from the website.
2. Settings opens on **Websites**, with the site under **Pending**.
3. Click **Allow**, then try again from the website.

You can **Deny** a site, or **Remove** an allowed/denied entry later. Only **Allowed** sites may print (unless you enable **Allow local scripts** for tools like curl — leave that off for normal browser use).

---

## 4. If nothing prints

Work through this short checklist:

| Check | What to do |
|-------|------------|
| Is RawLabelPrint running? | Look for the tray icon. Launch the app if it is missing. |
| Is a printer saved and set as default? | **Printers** — save at least one printer; set a default if the website does not name a printer. |
| Is the printer Online? | **Test print** or **↻**. Fix network/USB, then try again. |
| Is the website Allowed? | **Websites** — pending sites need **Allow**; **Denied** sites stay blocked until removed. |
| Did the request reach the app? | Enable **Capture recent print requests** on the **Prints** page, print again, and check **Recent prints**. |

**Platform tips**

- **macOS:** Allow RawLabelPrint under **System Settings → Privacy & Security → Local Network** if discovery or TCP to the printer fails.
- **Windows:** Allow the app through Windows Firewall (private networks) so printer search works.
- **USB:** Run **Search Zebra printers** after plugging in; the printer must appear as a CDC/ACM (serial/COM) device.

Still stuck? Ask your IT contact, or see the project README for technical detail.
