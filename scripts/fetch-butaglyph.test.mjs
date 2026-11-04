// node --test scripts/
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { assetUrl, binaryIn, sidecarName, targetFor, verify } from "./fetch-butaglyph.mjs";

const manifest = JSON.parse(readFileSync(new URL("./butaglyph.json", import.meta.url), "utf8"));

test("targetFor covers the packaged platforms only", () => {
  assert.equal(targetFor("linux", "x64"), "x86_64-unknown-linux-gnu");
  assert.equal(targetFor("win32", "x64"), "x86_64-pc-windows-msvc");
  assert.equal(targetFor("linux", "arm64"), null);
  assert.equal(targetFor("darwin", "x64"), null);
});

test("binary and sidecar names", () => {
  assert.equal(binaryIn("x86_64-unknown-linux-gnu"), "buta");
  assert.equal(binaryIn("x86_64-pc-windows-msvc"), "buta.exe");
  assert.equal(sidecarName("x86_64-unknown-linux-gnu"), "buta-x86_64-unknown-linux-gnu");
  assert.equal(sidecarName("x86_64-pc-windows-msvc"), "buta-x86_64-pc-windows-msvc.exe");
});

test("the manifest pins a hash and a file of its own version for every packaged target", () => {
  for (const target of [targetFor("linux", "x64"), targetFor("win32", "x64")]) {
    const a = manifest.assets[target];
    assert.ok(a, target);
    assert.match(a.sha256, /^[0-9a-f]{64}$/);
    assert.ok(a.file.startsWith(`buta-v${manifest.version}-`), a.file);
    assert.equal(
      assetUrl(manifest, a),
      `https://github.com/Huseynteymurzade28/butaglyph/releases/download/v${manifest.version}/${a.file}`,
    );
  }
});

test("verify accepts the right hash and refuses any other", () => {
  const sha = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"; // "hello"
  verify(Buffer.from("hello"), sha, "x");
  verify(Buffer.from("hello"), sha.toUpperCase(), "x");
  assert.throws(() => verify(Buffer.from("hellO"), sha, "x"), /SHA-256/);
});
