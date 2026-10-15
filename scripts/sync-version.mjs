#!/usr/bin/env node
/**
 * Keep app version in sync across package.json, tauri.conf.json, Cargo.toml, and Cargo.lock.
 *
 * Usage:
 *   node scripts/sync-version.mjs          # write package.json version to the other files
 *   node scripts/sync-version.mjs --check  # exit 1 if versions diverge
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const paths = {
  packageJson: join(root, "package.json"),
  tauriConf: join(root, "src-tauri", "tauri.conf.json"),
  cargoToml: join(root, "src-tauri", "Cargo.toml"),
  cargoLock: join(root, "src-tauri", "Cargo.lock"),
};

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function packageVersion() {
  return readJson(paths.packageJson).version;
}

function tauriVersion() {
  return readJson(paths.tauriConf).version;
}

function cargoTomlVersion() {
  const text = readFileSync(paths.cargoToml, "utf8");
  const match = text.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error(`No package version found in ${paths.cargoToml}`);
  }
  return match[1];
}

function cargoLockVersion() {
  const text = readFileSync(paths.cargoLock, "utf8");
  const match = text.match(
    /name = "rawlabelprint"\nversion = "([^"]+)"/,
  );
  if (!match) {
    throw new Error(`No rawlabelprint version found in ${paths.cargoLock}`);
  }
  return match[1];
}

function collectVersions() {
  return {
    "package.json": packageVersion(),
    "src-tauri/tauri.conf.json": tauriVersion(),
    "src-tauri/Cargo.toml": cargoTomlVersion(),
    "src-tauri/Cargo.lock": cargoLockVersion(),
  };
}

function check() {
  const versions = collectVersions();
  const expected = versions["package.json"];
  const mismatches = Object.entries(versions).filter(
    ([, version]) => version !== expected,
  );

  if (mismatches.length === 0) {
    console.log(`Versions in sync: ${expected}`);
    return;
  }

  console.error(`Version mismatch (expected ${expected} from package.json):`);
  for (const [file, version] of mismatches) {
    console.error(`  ${file}: ${version}`);
  }
  process.exit(1);
}

function sync() {
  const version = packageVersion();
  const versions = collectVersions();

  // Replace only the version field to preserve existing file formatting.
  const tauriConf = readFileSync(paths.tauriConf, "utf8").replace(
    /^(\s*"version"\s*:\s*")[^"]+(")/m,
    `$1${version}$2`,
  );
  writeFileSync(paths.tauriConf, tauriConf);

  const cargoToml = readFileSync(paths.cargoToml, "utf8").replace(
    /^version\s*=\s*"[^"]+"/m,
    `version = "${version}"`,
  );
  writeFileSync(paths.cargoToml, cargoToml);

  const cargoLock = readFileSync(paths.cargoLock, "utf8").replace(
    /(name = "rawlabelprint"\nversion = ")[^"]+(")/,
    `$1${version}$2`,
  );
  writeFileSync(paths.cargoLock, cargoLock);

  const changed = Object.entries(versions).filter(
    ([file, previous]) => file !== "package.json" && previous !== version,
  );

  if (changed.length === 0) {
    console.log(`Already in sync: ${version}`);
    return;
  }

  console.log(`Synced version ${version}:`);
  for (const [file, previous] of changed) {
    console.log(`  ${file}: ${previous} -> ${version}`);
  }
}

if (process.argv.includes("--check")) {
  check();
} else {
  sync();
}
