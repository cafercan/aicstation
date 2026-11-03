#!/usr/bin/env node
// Generates THIRD_PARTY_NOTICES.md from local metadata only (no network):
// - Rust crates: `cargo metadata --offline --locked --filter-platform
//   x86_64-pc-windows-msvc`, normal (non-dev, non-build) dependencies reachable
//   from the workspace members, license files from the local cargo registry;
// - UI packages: ui/package.json "dependencies" and their transitive
//   "dependencies" in ui/node_modules (what the Vite bundle can contain).
// Each license is printed in full once, followed by the packages under it
// and their copyright lines; Apache NOTICE files are reproduced verbatim and
// MPL-2.0 packages get a pointer to their source.
//
// Usage: node scripts/gen-notices.mjs [--check]
//   --check  exit 1 when THIRD_PARTY_NOTICES.md is not up to date.
// No dependencies beyond Node's standard library.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Licenses we may ship under, in order of preference when a package offers a choice. */
export const PREFERENCE = [
  "MIT", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "ISC", "Zlib", "Unicode-3.0",
  "0BSD", "MIT-0", "Unlicense", "CC0-1.0", "MPL-2.0",
];

const ALIASES = { "Apache-2.0 WITH LLVM-exception": "Apache-2.0", "Unicode-DFS-2016": "Unicode-3.0" };

function tokenize(expr) {
  // Old-style "MIT/Apache-2.0" and "Apache-2.0 / MIT" mean OR.
  const s = expr.replace(/\s*\/\s*/g, " OR ").replace(/\(/g, " ( ").replace(/\)/g, " ) ");
  const raw = s.split(/\s+/).filter(Boolean);
  // Re-join "X WITH Y" into one license token.
  const out = [];
  for (let i = 0; i < raw.length; i++) {
    if (raw[i + 1] === "WITH" && raw[i + 2]) {
      out.push(`${raw[i]} WITH ${raw[i + 2]}`);
      i += 2;
    } else out.push(raw[i]);
  }
  return out;
}

const rank = (set) => Math.max(...set.map((id) => PREFERENCE.indexOf(id)));

/**
 * The set of licenses a package must be distributed under, given its SPDX
 * expression: every AND operand, and for OR the most preferred alternative.
 * Throws for a license outside PREFERENCE.
 */
export function chooseLicenses(expr) {
  if (!expr || !expr.trim()) throw new Error("missing license expression");
  const toks = tokenize(expr);
  let pos = 0;
  const peek = () => toks[pos];
  function primary() {
    const t = toks[pos++];
    if (t === "(") {
      const v = orExpr();
      if (toks[pos++] !== ")") throw new Error(`unbalanced parentheses in "${expr}"`);
      return v;
    }
    if (t === undefined || t === ")" || t === "AND" || t === "OR") throw new Error(`bad license expression "${expr}"`);
    const id = ALIASES[t] ?? t;
    return PREFERENCE.includes(id) ? [[id]] : [];
  }
  // A value is a list of alternatives; each alternative is a list of ids (AND).
  function andExpr() {
    let alts = primary();
    while (peek() === "AND") {
      pos++;
      const rhs = primary();
      const next = [];
      for (const a of alts) for (const b of rhs) next.push([...new Set([...a, ...b])]);
      alts = next;
    }
    return alts;
  }
  function orExpr() {
    let alts = andExpr();
    while (peek() === "OR") {
      pos++;
      alts = alts.concat(andExpr());
    }
    return alts;
  }
  const alts = orExpr();
  if (pos !== toks.length) throw new Error(`bad license expression "${expr}"`);
  if (alts.length === 0) throw new Error(`no acceptable license in "${expr}"`);
  alts.sort((a, b) => rank(a) - rank(b) || a.length - b.length);
  return [...alts[0]].sort((a, b) => PREFERENCE.indexOf(a) - PREFERENCE.indexOf(b));
}

/** Copyright lines of a license/notice text, trimmed and de-duplicated. */
export function copyrightLines(text) {
  const out = [];
  for (const line of text.split(/\r?\n/)) {
    const l = line.trim();
    if (!/^(copyright\b|\(c\)|©)/i.test(l)) continue;
    // Skip template lines and the Apache appendix boilerplate.
    if (/\[yyyy\]|\{yyyy\}|<year>|\[name of copyright owner\]|copyright notice|copyright owner or|copyright statement/i.test(l)) continue;
    // Apache-2.0 body text that merely starts with the word.
    if (/^copyright license\b|^\(c\) you must\b/i.test(l)) continue;
    // A bare "Copyright:" heading names nobody.
    if (/^(copyright|\(c\)|©)\s*:?\s*$/i.test(l)) continue;
    if (!out.includes(l)) out.push(l);
  }
  return out;
}

const LICENSE_FILE = /^(licen[cs]e|copying|copyright|notice|unlicense)([-._].*)?$/i;

/** License-ish files directly in a package folder, sorted by name. */
export function licenseFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => LICENSE_FILE.test(f) && statSync(join(dir, f)).isFile())
    .sort()
    .map((f) => ({ name: f, text: readFileSync(join(dir, f), "utf8") }));
}

/** Rust packages reachable through normal dependencies from the workspace members. */
export function rustPackages(meta) {
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const members = new Set(meta.workspace_members);
  const seen = new Set();
  const stack = [...members];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const d of nodes.get(id)?.deps ?? []) {
      if (d.dep_kinds.some((k) => k.kind === null)) stack.push(d.pkg);
    }
  }
  return [...seen]
    .filter((id) => !members.has(id))
    .map((id) => byId.get(id))
    .map((p) => ({
      ecosystem: "rust",
      name: p.name,
      version: p.version,
      license: p.license ?? "",
      repository: p.repository ?? null,
      authors: p.authors ?? [],
      dir: dirname(p.manifest_path),
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/** npm packages reachable through "dependencies" from `uiDir`'s package.json. */
export function npmPackages(uiDir) {
  const root = JSON.parse(readFileSync(join(uiDir, "package.json"), "utf8"));
  const out = new Map();
  // Each entry: [package name, folder the require resolution starts from].
  const stack = Object.keys(root.dependencies ?? {}).map((n) => [n, uiDir]);
  while (stack.length) {
    const [name, from] = stack.pop();
    // Node resolution: nearest node_modules up from `from`, stopping at uiDir.
    let dir = null;
    for (let d = from; ; d = dirname(d)) {
      const cand = join(d, "node_modules", name);
      if (existsSync(join(cand, "package.json"))) { dir = cand; break; }
      if (resolve(d) === resolve(uiDir) || dirname(d) === d) break;
    }
    if (!dir) throw new Error(`npm package ${name} not installed (run npm --prefix ui ci)`);
    if (out.has(dir)) continue;
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const license = typeof pkg.license === "string" ? pkg.license : pkg.license?.type ?? "";
    const repo = typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url ?? null;
    const author = typeof pkg.author === "string" ? pkg.author : pkg.author?.name;
    out.set(dir, { ecosystem: "npm", name: pkg.name, version: pkg.version, license, repository: repo, authors: author ? [author] : [], dir });
    for (const dep of Object.keys(pkg.dependencies ?? {})) stack.push([dep, dir]);
  }
  return [...out.values()].sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/** Canonical full texts for licenses whose text does not vary per package. */
const TEMPLATES = {
  "MIT": `Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`,
  "BSD-3-Clause": `Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.`,
  "Zlib": `This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.
2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.
3. This notice may not be removed or altered from any source distribution.`,
};

/** Recognizes a package's file as the full text of `id` (for licenses without a template). */
const TEXT_MARKERS = {
  "Apache-2.0": [/Apache License/i, /Version 2\.0/i, /TERMS AND CONDITIONS FOR USE/i],
  "MPL-2.0": [/Mozilla Public License,? Version 2\.0/i, /Exhibit A/i],
  "Unicode-3.0": [/UNICODE LICENSE V3/i],
  "CC0-1.0": [/CC0 1\.0 Universal|Creative Commons Legal Code/i],
  "BSD-2-Clause": [/Redistribution and use in source and binary forms/i],
  "ISC": [/Permission to use, copy, modify, and\/or distribute/i],
  "0BSD": [/Permission to use, copy, modify, and\/or distribute/i],
  "MIT-0": [/Permission is hereby granted/i],
  "Unlicense": [/This is free and unencumbered software released into the public domain/i],
};

/** Full text of license `id`: the template, or the first package file that is that license. */
export function licenseText(id, packages) {
  if (TEMPLATES[id]) return TEMPLATES[id];
  const markers = TEXT_MARKERS[id];
  if (!markers) return null;
  for (const p of packages) {
    for (const f of p.files) {
      if (markers.every((m) => m.test(f.text))) return f.text.replace(/\r\n/g, "\n").trim();
    }
  }
  return null;
}

/** Source pointer for a package (MPL-2.0 requires one). */
export function sourcePointer(p) {
  const registry = p.ecosystem === "rust"
    ? `https://crates.io/crates/${p.name}/${p.version}`
    : `https://www.npmjs.com/package/${p.name}/v/${p.version}`;
  const repo = p.repository ? p.repository.replace(/^git\+/, "").replace(/\.git$/, "") : null;
  return repo ? `${registry} (source: ${repo})` : registry;
}

const fence = (text) => {
  const ticks = "`".repeat(Math.max(3, ...[...text.matchAll(/`+/g)].map((m) => m[0].length + 1)));
  return `${ticks}text\n${text}\n${ticks}`;
};

/**
 * The notices document. `packages` carry `ecosystem, name, version, license,
 * repository, authors, files` (files = [{name, text}]).
 */
export function render(packages) {
  const withChoice = packages.map((p) => ({ ...p, chosen: chooseLicenses(p.license) }));
  const ids = PREFERENCE.filter((id) => withChoice.some((p) => p.chosen.includes(id)));
  const lines = [];
  const push = (...l) => lines.push(...l);
  const rust = withChoice.filter((p) => p.ecosystem === "rust");
  const npm = withChoice.filter((p) => p.ecosystem === "npm");
  push(
    "# Third-party notices",
    "",
    "AI C-Station (MIT, see `LICENSE`) includes the third-party software listed",
    "below. This file is generated by `scripts/gen-notices.mjs` from local metadata",
    "(Cargo.lock / the local cargo registry for the x86_64-pc-windows-msvc target,",
    "and ui/node_modules for the UI bundle); do not edit it by hand.",
    "",
    "Where a package offers a choice of licenses, it is distributed here under the",
    "license(s) named in the \"Used\" column.",
    "",
    "## Summary",
    "",
    "| License | Packages |",
    "|---|---|",
    ...ids.map((id) => `| ${id} | ${withChoice.filter((p) => p.chosen.includes(id)).length} |`),
    "",
  );
  const table = (title, list) => {
    push(`## ${title} (${list.length})`, "", "| Package | Version | Declared license | Used |", "|---|---|---|---|");
    for (const p of list) push(`| ${p.name} | ${p.version} | ${p.license} | ${p.chosen.join(" AND ")} |`);
    push("");
  };
  table("Rust crates", rust);
  table("UI packages (npm)", npm);

  const mpl = withChoice.filter((p) => p.chosen.includes("MPL-2.0"));
  if (mpl.length) {
    push(
      "## MPL-2.0 source code",
      "",
      "The following packages are used unmodified under the Mozilla Public License 2.0.",
      "Their source code is available at:",
      "",
      ...mpl.map((p) => `- ${p.name} ${p.version}: ${sourcePointer(p)}`),
      "",
    );
  }

  push("## License texts", "");
  for (const id of ids) {
    const under = withChoice.filter((p) => p.chosen.includes(id));
    const text = licenseText(id, under);
    push(`### ${id}`, "", "Packages:", "");
    for (const p of under) {
      const own = p.files.flatMap((f) => copyrightLines(f.text));
      const who = own.length ? own : p.authors.length ? [`Authors: ${p.authors.join(", ")}`] : [];
      push(`- ${p.name} ${p.version}${who.length ? ` — ${[...new Set(who)].join("; ")}` : ""}`);
    }
    push("");
    if (text) {
      push(fence(text), "");
    } else {
      push(`Full text: https://spdx.org/licenses/${id}.html`, "");
    }
  }

  const notices = withChoice.flatMap((p) =>
    p.files.filter((f) => /^notice/i.test(f.name)).map((f) => ({ p, text: f.text.replace(/\r\n/g, "\n").trim() })));
  if (notices.length) {
    push("## NOTICE files", "");
    for (const { p, text } of notices) push(`### ${p.name} ${p.version}`, "", fence(text), "");
  }
  return lines.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}

function main() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const meta = JSON.parse(execFileSync("cargo", [
    "metadata", "--format-version", "1", "--offline", "--locked", "--filter-platform", "x86_64-pc-windows-msvc",
  ], { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 }));
  const pkgs = [...rustPackages(meta), ...npmPackages(join(root, "ui"))].map((p) => ({ ...p, files: licenseFiles(p.dir) }));
  const out = render(pkgs);
  const target = join(root, "THIRD_PARTY_NOTICES.md");
  if (process.argv.includes("--check")) {
    const cur = existsSync(target) ? readFileSync(target, "utf8").replace(/\r\n/g, "\n") : "";
    if (cur !== out) {
      console.error("THIRD_PARTY_NOTICES.md is out of date: run node scripts/gen-notices.mjs");
      process.exit(1);
    }
    console.log("THIRD_PARTY_NOTICES.md is up to date");
    return;
  }
  writeFileSync(target, out);
  console.log(`wrote ${target} (${pkgs.length} packages)`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
