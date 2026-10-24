#!/usr/bin/env node
/**
 * Render docs/getting-started.md → docs/getting-started.html for the tray help item.
 *
 * Usage:
 *   node scripts/render-getting-started.mjs          # write HTML
 *   node scripts/render-getting-started.mjs --check  # exit 1 if committed HTML is stale
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { marked } from "marked";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const mdPath = join(root, "docs", "getting-started.md");
const htmlPath = join(root, "docs", "getting-started.html");
const checkOnly = process.argv.includes("--check");

marked.setOptions({ gfm: true });

function escapeAttr(s) {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}

function renderHtml(markdown) {
  const body = marked.parse(markdown, { async: false });
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Getting Started — RawLabelPrint</title>
  <style>
    :root {
      color-scheme: light dark;
      --fg: #1a1a1a;
      --muted: #555;
      --border: #ccc;
      --bg: #fff;
      --code-bg: #f4f4f4;
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --fg: #e8e8e8;
        --muted: #aaa;
        --border: #444;
        --bg: #1c1c1c;
        --code-bg: #2a2a2a;
      }
    }
    * { box-sizing: border-box; }
    body {
      margin: 0 auto;
      padding: 1.5rem 1.25rem 3rem;
      max-width: 42rem;
      font: 16px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif;
      color: var(--fg);
      background: var(--bg);
    }
    h1 { font-size: 1.75rem; line-height: 1.25; margin: 0 0 0.75rem; }
    h2 { font-size: 1.2rem; margin: 1.75rem 0 0.5rem; }
    p, li { margin: 0.4rem 0; }
    ul, ol { padding-left: 1.25rem; }
    hr { border: none; border-top: 1px solid var(--border); margin: 1.5rem 0; }
    strong { font-weight: 600; }
    code {
      font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      font-size: 0.9em;
      background: var(--code-bg);
      padding: 0.1em 0.35em;
      border-radius: 3px;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      margin: 0.75rem 0 1rem;
      font-size: 0.95rem;
    }
    th, td {
      border: 1px solid var(--border);
      padding: 0.45rem 0.6rem;
      text-align: left;
      vertical-align: top;
    }
    th { font-weight: 600; }
    .source {
      margin-top: 2rem;
      font-size: 0.85rem;
      color: var(--muted);
    }
    @media print {
      body { max-width: none; padding: 0; color: #000; background: #fff; }
      .source { display: none; }
    }
  </style>
</head>
<body>
${body}
<p class="source">Generated from <code>${escapeAttr("docs/getting-started.md")}</code>.</p>
</body>
</html>
`;
}

const markdown = readFileSync(mdPath, "utf8");
const html = renderHtml(markdown);

if (checkOnly) {
  let existing;
  try {
    existing = readFileSync(htmlPath, "utf8");
  } catch {
    console.error(`Missing ${htmlPath}. Run: npm run docs:getting-started`);
    process.exit(1);
  }
  if (existing !== html) {
    console.error(
      "docs/getting-started.html is out of date. Run: npm run docs:getting-started",
    );
    process.exit(1);
  }
  console.log("docs/getting-started.html is up to date");
  process.exit(0);
}

writeFileSync(htmlPath, html);
console.log(`Wrote ${htmlPath}`);
