// Assembles the latest.json manifest tauri-plugin-updater checks against
// (see src-tauri/tauri.conf.json's plugins.updater.endpoints) from a signed
// npm run tauri build's output. Run this after building with
// TAURI_SIGNING_PRIVATE_KEY set — see docs/RELEASING.md for the full
// release checklist.
//
// Usage: node scripts/make-latest-json.mjs "Release notes go here"

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

const GITHUB_REPO = "johnnyklau/Fairy";

function readVersion() {
  const conf = JSON.parse(
    readFileSync(path.join(repoRoot, "src-tauri/tauri.conf.json"), "utf-8"),
  );
  return { version: conf.version, productName: conf.productName };
}

// The custom CARGO_TARGET_DIR src-tauri/.cargo/config.toml points builds
// at (see the portability note there) — parsed rather than hardcoded, so
// this keeps working if that path ever changes.
function readCargoTargetDir() {
  const toml = readFileSync(
    path.join(repoRoot, "src-tauri/.cargo/config.toml"),
    "utf-8",
  );
  const match = toml.match(/target-dir\s*=\s*"([^"]+)"/);
  if (!match) {
    throw new Error(
      "Could not find target-dir in src-tauri/.cargo/config.toml",
    );
  }
  return match[1];
}

function main() {
  const notes = process.argv[2] ?? "";
  const { version, productName } = readVersion();
  const targetDir = readCargoTargetDir();

  const installerName = `${productName}_${version}_x64-setup.exe`;
  const nsisDir = path.join(targetDir, "release", "bundle", "nsis");
  const sigPath = path.join(nsisDir, `${installerName}.sig`);

  let signature;
  try {
    signature = readFileSync(sigPath, "utf-8").trim();
  } catch {
    throw new Error(
      `Could not read ${sigPath} — build with TAURI_SIGNING_PRIVATE_KEY set first (see docs/RELEASING.md).`,
    );
  }

  const manifest = {
    version,
    notes,
    pub_date: new Date().toISOString(),
    platforms: {
      "windows-x86_64": {
        signature,
        url: `https://github.com/${GITHUB_REPO}/releases/download/v${version}/${installerName}`,
      },
    },
  };

  const outPath = path.join(repoRoot, "latest.json");
  writeFileSync(outPath, JSON.stringify(manifest, null, 2) + "\n");

  console.log(`Wrote ${outPath}`);
  console.log(
    `Upload it alongside ${installerName} and the .msi to the GitHub Release tagged v${version}.`,
  );
}

main();
