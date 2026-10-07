#!/usr/bin/env node
// bump-version.mjs — Prepara una release en un solo paso:
//
//   node scripts/bump-version.mjs 0.4.0
//
// Sube la versión en los cinco ficheros que la declaran y convierte la sección
// `## [Unreleased]` del CHANGELOG en `## [0.4.0] — <hoy>` (sus notas acaban en el
// `latest.json` y la app las muestra como "Novedades"). Después: revisar el diff,
// commit, `git tag v0.4.0` y `git push origin main v0.4.0`.

import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { SEMVER, VERSION_FILES, readers, writers, releaseChangelog } from "./release-lib.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = (process.argv[2] || "").replace(/^v/, "");
if (!SEMVER.test(version)) {
  console.error("Uso: node scripts/bump-version.mjs <X.Y.Z>");
  process.exit(1);
}

const read = (f) => readFileSync(resolve(root, f), "utf8");
const current = readers["src-tauri/tauri.conf.json"](read("src-tauri/tauri.conf.json"));

// Primero se calcula TODO y solo después se escribe: o cambian todos los
// ficheros o ninguno.
const out = {};
for (const file of VERSION_FILES) {
  const updated = writers[file](read(file), version);
  if (readers[file](updated) !== version) {
    console.error(`No se pudo actualizar la versión en ${file}`);
    process.exit(1);
  }
  out[file] = updated;
}
// Fecha LOCAL (no UTC): es la del día en que se prepara la release.
const d = new Date();
const pad = (n) => String(n).padStart(2, "0");
const today = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
try {
  out["CHANGELOG.md"] = releaseChangelog(read("CHANGELOG.md"), version, today);
} catch (e) {
  console.error(`CHANGELOG.md: ${e.message}`);
  process.exit(1);
}

for (const [file, txt] of Object.entries(out)) writeFileSync(resolve(root, file), txt);
console.log(`Versión ${current} → ${version} en ${VERSION_FILES.length} ficheros y CHANGELOG.`);
console.log(`Siguiente: commit, git tag v${version} && git push origin main v${version}`);
