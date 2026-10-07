#!/usr/bin/env node
// check-release.mjs — Guardia del workflow de release:
//
//   node scripts/check-release.mjs v0.4.0 [notas.md]
//
// Falla si alguno de los ficheros de versión no coincide con la etiqueta (la
// release anunciaría una versión equivocada en `latest.json` y nadie se
// actualizaría) o si el CHANGELOG no tiene sección para esa versión. Si se pasa
// una ruta, escribe ahí las notas de la versión para la release / `latest.json`.

import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { SEMVER, VERSION_FILES, versionMismatches, changelogSection } from "./release-lib.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = (process.argv[2] || "").replace(/^v/, "");
if (!SEMVER.test(version)) {
  console.error("Uso: node scripts/check-release.mjs <vX.Y.Z> [notas.md]");
  process.exit(1);
}

const contents = Object.fromEntries(
  VERSION_FILES.map((f) => [f, readFileSync(resolve(root, f), "utf8")]),
);
const errors = versionMismatches(contents, version);
const notes = changelogSection(readFileSync(resolve(root, "CHANGELOG.md"), "utf8"), version);
if (!notes) errors.push(`CHANGELOG.md: falta la sección "## [${version}]" (o está vacía)`);

if (errors.length) {
  console.error(`La etiqueta v${version} no cuadra con el código:`);
  for (const e of errors) console.error(`  - ${e}`);
  console.error("Usa `node scripts/bump-version.mjs <X.Y.Z>` antes de etiquetar.");
  process.exit(1);
}
if (process.argv[3]) writeFileSync(process.argv[3], notes + "\n");
console.log(`v${version}: versiones y CHANGELOG en orden.`);
