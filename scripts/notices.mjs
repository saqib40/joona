// Writes target/notices/THIRD_PARTY_LICENSES.html with the licence of every
// Rust crate and npm package that goes into a release. Release builds install
// it next to Joona, and Settings > About opens it.
//
//   cargo install cargo-about --locked --features cli
//   node scripts/notices.mjs

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "target", "notices");
const out = join(dir, "THIRD_PARTY_LICENSES.html");
const crateList = join(dir, "crates.json");

mkdirSync(dir, { recursive: true });
// cargo-about won't write to a redirected stdout under PowerShell, so it gets a file.
execFileSync("cargo", ["about", "generate", "--format", "json", "--workspace", "--fail", "--locked", "-o", crateList], {
  cwd: root,
  stdio: "inherit",
});
const rust = JSON.parse(readFileSync(crateList, "utf8"));
const crates = rust.licenses.map((licence) => ({
  name: licence.name,
  text: licence.text,
  users: licence.used_by.map(({ crate }) => ({
    name: crate.name,
    version: crate.version,
    url: crate.repository || `https://crates.io/crates/${crate.name}`,
  })),
}));

const packages = npmPackages(join(root, "apps", "desktop"));
const byText = new Map();
for (const pkg of packages) {
  const key = pkg.text ?? `${pkg.licence} (no licence file)`;
  const group = byText.get(key) ?? { name: pkg.licence, text: pkg.text, users: [] };
  group.users.push({ name: pkg.name, version: pkg.version, url: pkg.url });
  byText.set(key, group);
}
const npm = [...byText.values()].sort((a, b) => a.name.localeCompare(b.name));

writeFileSync(out, page([...crates, ...npm]));
console.log(`Wrote ${out}: ${rust.crates.length} crates and ${packages.length} npm packages.`);

/** The packages the app's UI is built from, not its build tools. */
function npmPackages(app) {
  const seen = new Map();
  const visit = (dir, deps) => {
    for (const name of deps) {
      const found = resolve(name, dir);
      if (!found || seen.has(found)) continue;
      const manifest = JSON.parse(readFileSync(join(found, "package.json"), "utf8"));
      seen.set(found, describe(found, manifest));
      visit(found, Object.keys({ ...manifest.dependencies, ...manifest.optionalDependencies }));
    }
  };
  const manifest = JSON.parse(readFileSync(join(app, "package.json"), "utf8"));
  visit(app, Object.keys(manifest.dependencies ?? {}));
  return [...seen.values()].sort((a, b) => a.name.localeCompare(b.name));
}

/** Node's lookup: `node_modules` here, then in each folder above. */
function resolve(name, from) {
  for (let dir = from; ; dir = dirname(dir)) {
    const candidate = join(dir, "node_modules", name);
    if (existsSync(join(candidate, "package.json"))) return candidate;
    if (dirname(dir) === dir) return null;
  }
}

function describe(dir, manifest) {
  const file = readdirSync(dir).find((f) => /^(licen[cs]e|copying)(\.|-|$)/i.test(f));
  const repository = typeof manifest.repository === "string" ? manifest.repository : manifest.repository?.url;
  return {
    name: manifest.name,
    version: manifest.version,
    licence: (typeof manifest.license === "string" ? manifest.license : manifest.license?.type) ?? "Unknown",
    text: file ? readFileSync(join(dir, file), "utf8").trim() : null,
    url: link(repository) ?? manifest.homepage ?? `https://www.npmjs.com/package/${manifest.name}`,
  };
}

/** A web address for the forms `repository` takes, such as `owner/repo`. */
function link(repository) {
  if (!repository) return null;
  const url = repository
    .replace(/^git\+/, "")
    .replace(/^(git@github\.com:|github:)/, "https://github.com/")
    .replace(/^git:\/\//, "https://")
    .replace(/\.git$/, "");
  if (/^[\w.-]+\/[\w.-]+$/.test(url)) return `https://github.com/${url}`;
  return /^https?:\/\//.test(url) ? url : null;
}

function page(licences) {
  const sections = licences
    .map(
      (licence) => `<section>
  <h2>${escape(licence.name)}</h2>
  <p>${licence.users.map((u) => `<a href="${escape(u.url)}">${escape(u.name)} ${escape(u.version)}</a>`).join(", ")}</p>
  ${licence.text ? `<pre>${escape(licence.text)}</pre>` : "<p>The package doesn't include its licence text.</p>"}
</section>`,
    )
    .join("\n");
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Joona: third-party licences</title>
<style>
  body { margin: 0 auto; max-width: 880px; padding: 40px 24px; font: 14px/1.55 "Segoe UI", system-ui, sans-serif; color: #1f1d1a; background: #f8f6f1; }
  h1 { font-size: 26px; font-weight: 600; margin: 0 0 6px; }
  h2 { font-size: 15px; margin: 0 0 6px; }
  section { margin: 18px 0; padding: 18px 20px; border-radius: 12px; background: #fff; box-shadow: 0 1px 2px rgb(0 0 0 / 0.06); }
  a { color: #8a5a00; }
  pre { margin: 12px 0 0; white-space: pre-wrap; font: 12px/1.5 Consolas, monospace; color: #4a463f; }
  @media (prefers-color-scheme: dark) {
    body { color: #ece8e1; background: #171614; }
    section { background: #22201d; box-shadow: none; }
    a { color: #f0b54a; }
    pre { color: #bdb6ab; }
  }
</style>
</head>
<body>
<h1>Third-party licences</h1>
<p>Joona is built with the open-source software below. The speech and AI models you can download aren't part of it; their licences are listed in <a href="https://github.com/saqib40/joona/blob/main/THIRD_PARTY_NOTICES.md">THIRD_PARTY_NOTICES.md</a>.</p>
${sections}
</body>
</html>
`;
}

function escape(text) {
  return String(text).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
}
