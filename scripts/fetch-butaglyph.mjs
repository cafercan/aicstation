#!/usr/bin/env node
// Downloads the butaglyph (`buta`) release pinned in scripts/butaglyph.json,
// checks its SHA-256 and puts the binary where the packages take it from:
//   src-tauri/binaries/buta-<target triple>[.exe]
// (the name Tauri expects for a sidecar, see scripts/build-linux.sh; the
// Windows installer copies it as buta.exe). To move to a newer butaglyph,
// change the version, file names and hashes in butaglyph.json.
//
// Usage: node scripts/fetch-butaglyph.mjs [<target triple>]
//   default target: this machine (x86_64 Linux or Windows).
// Needs `tar`: on PATH on Linux; on Windows the one in System32 (Windows 10+),
// which also reads .zip.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** The target triple of `platform`/`arch` (Node's names), or null when not packaged. */
export function targetFor(platform, arch) {
  if (arch !== "x64") return null;
  return { linux: "x86_64-unknown-linux-gnu", win32: "x86_64-pc-windows-msvc" }[platform] ?? null;
}

/** File name of the binary inside the release archive. */
export const binaryIn = (target) => (target.includes("windows") ? "buta.exe" : "buta");

/** File name Tauri expects for the sidecar of `target`. */
export const sidecarName = (target) => `buta-${target}${target.includes("windows") ? ".exe" : ""}`;

/** Download URL of `asset` from the pinned release. */
export const assetUrl = (manifest, asset) =>
  `${manifest.repository}/releases/download/v${manifest.version}/${asset.file}`;

/** Throws unless `bytes` hash to `sha256` (hex). */
export function verify(bytes, sha256, name) {
  const got = createHash("sha256").update(bytes).digest("hex");
  if (got !== sha256.toLowerCase()) throw new Error(`${name}: SHA-256 ${got}, expected ${sha256}`);
}

async function main() {
  const manifest = JSON.parse(readFileSync(join(root, "scripts", "butaglyph.json"), "utf8"));
  const target = process.argv[2] ?? targetFor(process.platform, process.arch);
  const asset = target && manifest.assets[target];
  if (!asset) throw new Error(`no butaglyph build for ${target ?? `${process.platform}-${process.arch}`}`);

  const url = assetUrl(manifest, asset);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  const bytes = Buffer.from(await res.arrayBuffer());
  verify(bytes, asset.sha256, asset.file);

  const tmp = mkdtempSync(join(tmpdir(), "buta-"));
  try {
    const archive = join(tmp, asset.file);
    writeFileSync(archive, bytes);
    // Windows' own tar (bsdtar) reads .zip; a GNU tar earlier in PATH (Git's) does not.
    const tar = process.platform === "win32"
      ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe")
      : "tar";
    execFileSync(tar, ["-xf", archive, "-C", tmp, binaryIn(target)], { stdio: "inherit" });
    const out = join(root, "src-tauri", "binaries", sidecarName(target));
    mkdirSync(dirname(out), { recursive: true });
    copyFileSync(join(tmp, binaryIn(target)), out);
    chmodSync(out, 0o755);
    console.log(`butaglyph ${manifest.version} -> ${out}`);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => {
    console.error(e.message);
    process.exit(1);
  });
}
