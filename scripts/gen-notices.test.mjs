// node --test scripts/
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { chooseLicenses, copyrightLines, licenseFiles, npmPackages, render, rustPackages, sourcePointer } from "./gen-notices.mjs";

test("chooseLicenses picks the preferred alternative and keeps every AND operand", () => {
  assert.deepEqual(chooseLicenses("MIT OR Apache-2.0"), ["MIT"]);
  assert.deepEqual(chooseLicenses("Apache-2.0 OR MIT"), ["MIT"]);
  assert.deepEqual(chooseLicenses("MIT/Apache-2.0"), ["MIT"]);
  assert.deepEqual(chooseLicenses("Apache-2.0 / MIT"), ["MIT"]);
  assert.deepEqual(chooseLicenses("(MIT OR Apache-2.0) AND Unicode-3.0"), ["MIT", "Unicode-3.0"]);
  assert.deepEqual(chooseLicenses("BSD-3-Clause AND MIT"), ["MIT", "BSD-3-Clause"]);
  assert.deepEqual(chooseLicenses("BSD-2-Clause OR Apache-2.0"), ["Apache-2.0"]);
  assert.deepEqual(chooseLicenses("CC0-1.0 OR MIT-0 OR Apache-2.0"), ["Apache-2.0"]);
  assert.deepEqual(chooseLicenses("Unlicense OR MIT"), ["MIT"]);
  assert.deepEqual(chooseLicenses("MPL-2.0"), ["MPL-2.0"]);
  assert.deepEqual(chooseLicenses("Apache-2.0 WITH LLVM-exception OR MIT"), ["MIT"]);
});

test("chooseLicenses refuses unknown, copyleft or malformed expressions", () => {
  for (const bad of ["", "GPL-3.0-only", "LGPL-2.1 AND MIT", "(MIT", "MIT OR", "UNKNOWN"]) {
    assert.throws(() => chooseLicenses(bad), undefined, bad);
  }
  // A copyleft alternative is fine when a permissive one is offered.
  assert.deepEqual(chooseLicenses("GPL-2.0 OR MIT"), ["MIT"]);
});

test("copyrightLines keeps real holders and drops template lines", () => {
  const text = [
    "MIT License",
    "",
    "Copyright (c) 2017 Jane Doe",
    "  Copyright 2020 The Demo Authors  ",
    "Copyright (c) 2017 Jane Doe",
    "(c) 2021 Someone",
    "© 2022 Another",
    "   Copyright [yyyy] [name of copyright owner]",
    "The above copyright notice and this permission notice shall be included",
    "      copyright license to reproduce, prepare Derivative Works of,",
    "      (c) You must retain, in the Source form of any Derivative Works",
    "copyright:",
  ].join("\n");
  assert.deepEqual(copyrightLines(text), [
    "Copyright (c) 2017 Jane Doe", "Copyright 2020 The Demo Authors", "(c) 2021 Someone", "© 2022 Another",
  ]);
});

test("licenseFiles finds LICENSE-*, COPYING, NOTICE and UNLICENSE only", () => {
  const d = mkdtempSync(join(tmpdir(), "notices-"));
  for (const f of ["LICENSE-MIT", "LICENSE-APACHE", "COPYING", "NOTICE", "UNLICENSE", "README.md", "license.txt", "src.rs"]) {
    writeFileSync(join(d, f), f);
  }
  mkdirSync(join(d, "LICENSES"));
  assert.deepEqual(licenseFiles(d).map((f) => f.name), ["COPYING", "LICENSE-APACHE", "LICENSE-MIT", "NOTICE", "UNLICENSE", "license.txt"]);
  assert.deepEqual(licenseFiles(join(d, "missing")), []);
});

test("rustPackages follows normal dependencies only and skips workspace members", () => {
  const dep = (pkg, kind) => ({ name: pkg, pkg, dep_kinds: [{ kind, target: null }] });
  const meta = {
    workspace_members: ["app", "core"],
    packages: ["app", "core", "a", "b", "build-only", "dev-only", "c"].map((id) => ({
      id, name: id, version: "1.0.0", license: "MIT", repository: null, authors: [], manifest_path: `/r/${id}/Cargo.toml`,
    })),
    resolve: {
      nodes: [
        { id: "app", deps: [dep("core", null), dep("build-only", "build"), dep("dev-only", "dev")] },
        { id: "core", deps: [dep("a", null)] },
        { id: "a", deps: [dep("b", null)] },
        { id: "b", deps: [] },
        { id: "build-only", deps: [dep("c", null)] },
        { id: "dev-only", deps: [] },
        { id: "c", deps: [] },
      ],
    },
  };
  assert.deepEqual(rustPackages(meta).map((p) => p.name), ["a", "b"]);
});

test("npmPackages walks dependencies (not devDependencies) with nested node_modules", () => {
  const ui = mkdtempSync(join(tmpdir(), "notices-ui-"));
  const pkg = (dir, json) => { mkdirSync(dir, { recursive: true }); writeFileSync(join(dir, "package.json"), JSON.stringify(json)); };
  pkg(ui, { name: "ui", dependencies: { a: "1" }, devDependencies: { tool: "1" } });
  pkg(join(ui, "node_modules", "a"), { name: "a", version: "1.0.0", license: "MIT", dependencies: { b: "1" } });
  pkg(join(ui, "node_modules", "a", "node_modules", "b"), { name: "b", version: "2.0.0", license: "ISC" });
  pkg(join(ui, "node_modules", "b"), { name: "b", version: "1.0.0", license: "MIT" });
  pkg(join(ui, "node_modules", "tool"), { name: "tool", version: "9.0.0", license: "GPL-3.0" });
  const got = npmPackages(ui).map((p) => `${p.name}@${p.version}`);
  assert.deepEqual(got, ["a@1.0.0", "b@2.0.0"], "nested copy wins; dev tools excluded");
});

test("render: summary, tables, one text per license, copyrights, MPL source pointer", () => {
  const p = (name, license, files = [], extra = {}) => ({
    ecosystem: "rust", name, version: "1.0.0", license, repository: null, authors: [], files, ...extra,
  });
  const apache = "Apache License\nVersion 2.0, January 2004\nTERMS AND CONDITIONS FOR USE, REPRODUCTION\n...";
  const doc = render([
    p("alpha", "MIT OR Apache-2.0", [{ name: "LICENSE-MIT", text: "Copyright (c) 2020 Alpha Devs\n" }]),
    p("beta", "Apache-2.0", [{ name: "LICENSE", text: apache }, { name: "NOTICE", text: "Beta notice text" }]),
    p("gamma", "MPL-2.0", [], { repository: "https://github.com/x/gamma", authors: ["G <g@x>"] }),
    { ...p("delta", "MIT"), ecosystem: "npm" },
  ]);
  assert.match(doc, /^# Third-party notices/);
  assert.match(doc, /\| MIT \| 2 \|/);
  assert.match(doc, /\| alpha \| 1\.0\.0 \| MIT OR Apache-2\.0 \| MIT \|/);
  assert.match(doc, /## UI packages \(npm\) \(1\)/);
  assert.equal(doc.match(/Permission is hereby granted, free of charge/g).length, 1, "MIT text once");
  assert.equal(doc.match(/TERMS AND CONDITIONS FOR USE/g).length, 1, "Apache text once, from the package file");
  assert.match(doc, /- alpha 1\.0\.0 — Copyright \(c\) 2020 Alpha Devs/);
  assert.match(doc, /- gamma 1\.0\.0 — Authors: G <g@x>/);
  assert.match(doc, /## MPL-2\.0 source code[\s\S]*gamma 1\.0\.0: https:\/\/crates\.io\/crates\/gamma\/1\.0\.0 \(source: https:\/\/github\.com\/x\/gamma\)/);
  assert.match(doc, /Full text: https:\/\/spdx\.org\/licenses\/MPL-2\.0\.html/, "no local MPL text -> pointer");
  assert.match(doc, /## NOTICE files[\s\S]*### beta 1\.0\.0[\s\S]*Beta notice text/);
});

test("sourcePointer cleans git URLs and covers npm", () => {
  assert.equal(
    sourcePointer({ ecosystem: "npm", name: "x", version: "1.2.3", repository: "git+https://github.com/o/x.git" }),
    "https://www.npmjs.com/package/x/v/1.2.3 (source: https://github.com/o/x)");
  assert.equal(sourcePointer({ ecosystem: "rust", name: "y", version: "0.1.0", repository: null }), "https://crates.io/crates/y/0.1.0");
});
