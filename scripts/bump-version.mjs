#!/usr/bin/env node
// Set the Nexus Desktop version in the four places that have to agree.
//
//   node scripts/bump-version.mjs 0.1.1
//
// package.json and package-lock.json keep npm happy, tauri.conf.json is what
// the installer and the update manifest are named for, and Cargo.toml is what
// the running app reports in the tray and the About box. A release where these
// disagree produces an installer the updater will not recognise, so this script
// is the only supported way to change them.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!version) {
  console.error("usage: node scripts/bump-version.mjs <version>");
  process.exit(2);
}
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`not a version: ${version} (expected MAJOR.MINOR.PATCH)`);
  process.exit(2);
}

/** Read, transform, write, and say what changed. Never writes LF as CRLF. */
function edit(relativePath, transform) {
  const path = join(root, relativePath);
  const before = readFileSync(path, "utf8");
  const after = transform(before);
  if (after === before) {
    console.log(`  ${relativePath}: already ${version}`);
    return;
  }
  writeFileSync(path, after, "utf8");
  console.log(`  ${relativePath}: ${version}`);
}

function setJsonVersion(source, pointers) {
  const document = JSON.parse(source);
  for (const pointer of pointers) {
    let node = document;
    for (const key of pointer.slice(0, -1)) {
      if (node?.[key] === undefined) {
        node = undefined;
        break;
      }
      node = node[key];
    }
    if (node !== undefined) {
      node[pointer.at(-1)] = version;
    }
  }
  // Two spaces and a trailing newline: what npm and the Tauri CLI both write.
  return `${JSON.stringify(document, null, 2)}\n`;
}

console.log(`Nexus Desktop -> v${version}`);

edit("package.json", (source) => setJsonVersion(source, [["version"]]));

edit("package-lock.json", (source) =>
  setJsonVersion(source, [["version"], ["packages", "", "version"]]),
);

edit("src-tauri/tauri.conf.json", (source) => setJsonVersion(source, [["version"]]));

edit("src-tauri/Cargo.toml", (source) => {
  // Only the version of the package itself, which is the first `version =` in
  // the `[package]` table. Dependency versions further down are left alone.
  let inPackage = false;
  let replaced = false;
  return source
    .split("\n")
    .map((line) => {
      if (line.startsWith("[")) {
        inPackage = line.trim() === "[package]";
        return line;
      }
      if (inPackage && !replaced && /^version\s*=/.test(line)) {
        replaced = true;
        return `version = "${version}"`;
      }
      return line;
    })
    .join("\n");
});

console.log("Next: commit, then tag desktop-vX.Y.Z with a message, then push the tag.");
