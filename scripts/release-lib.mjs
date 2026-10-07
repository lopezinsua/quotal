// release-lib.mjs — Lógica PURA del proceso de release (sin IO), compartida por
// `bump-version.mjs` y `check-release.mjs` y cubierta por tests.
//
// La versión de Quotal vive en CINCO sitios que deben coincidir con la etiqueta
// `vX.Y.Z`: package.json, package-lock.json, Cargo.toml, Cargo.lock y
// tauri.conf.json. El `latest.json` del auto-updater toma la de tauri.conf.json:
// si se etiqueta sin subirla, la release anuncia la versión VIEJA y ninguna app
// instalada se actualiza. Por eso el workflow de release las comprueba todas.

export const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const sectionTitle = (version) => new RegExp(`^## \\[${escapeRe(version)}\\]`, "m");

/// Versión declarada en cada fichero (texto → versión o null).
export const readers = {
  "package.json": (txt) => JSON.parse(txt).version ?? null,
  "package-lock.json": (txt) => {
    const j = JSON.parse(txt);
    return j.version === j.packages?.[""]?.version ? (j.version ?? null) : null;
  },
  "src-tauri/tauri.conf.json": (txt) => JSON.parse(txt).version ?? null,
  "src-tauri/Cargo.toml": (txt) => txt.match(/^\[package\][^[]*?^version\s*=\s*"([^"]+)"/m)?.[1] ?? null,
  "src-tauri/Cargo.lock": (txt) =>
    txt.match(/\[\[package\]\]\r?\nname = "quotal"\r?\nversion = "([^"]+)"/)?.[1] ?? null,
};

/// Reescribe la versión de cada fichero CONSERVANDO su formato (solo se cambia
/// el valor; en los JSON, sobre el texto, para no reordenar ni reindentar).
export const writers = {
  "package.json": (txt, v) => txt.replace(/^(\s*"version"\s*:\s*")[^"]*(")/m, `$1${v}$2`),
  "package-lock.json": (txt, v) =>
    txt
      // raíz
      .replace(/^(\s{2}"version"\s*:\s*")[^"]*(")/m, `$1${v}$2`)
      // packages[""]
      .replace(/("packages"\s*:\s*\{\s*""\s*:\s*\{[^}]*?"version"\s*:\s*")[^"]*(")/, `$1${v}$2`),
  "src-tauri/tauri.conf.json": (txt, v) =>
    txt.replace(/^(\s*"version"\s*:\s*")[^"]*(")/m, `$1${v}$2`),
  "src-tauri/Cargo.toml": (txt, v) =>
    txt.replace(/^(\[package\][^[]*?^version\s*=\s*")[^"]*(")/m, `$1${v}$2`),
  "src-tauri/Cargo.lock": (txt, v) =>
    txt.replace(/(\[\[package\]\]\r?\nname = "quotal"\r?\nversion = ")[^"]*(")/, `$1${v}$2`),
};

export const VERSION_FILES = Object.keys(readers);

/// Cuerpo de la sección `## [version]` del CHANGELOG (sin el título), o null.
/// Termina en el siguiente `## [` o en las referencias de enlaces del final.
export function changelogSection(md, version) {
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  const title = sectionTitle(version);
  const start = lines.findIndex((l) => title.test(l));
  if (start < 0) return null;
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((l) => /^## \[/.test(l) || /^\[[^\]]+\]:\s/.test(l));
  const body = (end < 0 ? rest : rest.slice(0, end)).join("\n").trim();
  return body || null;
}

/// Convierte `## [Unreleased]` en `## [version] — fecha` y deja encima una
/// sección `Unreleased` vacía para lo siguiente. Error si no hay nada pendiente
/// o si esa versión ya existe.
export function releaseChangelog(md, version, date) {
  const nl = md.includes("\r\n") ? "\r\n" : "\n";
  if (!sectionTitle("Unreleased").test(md)) throw new Error("CHANGELOG sin sección [Unreleased]");
  if (!changelogSection(md, "Unreleased")) throw new Error("[Unreleased] está vacía");
  if (sectionTitle(version).test(md)) throw new Error(`el CHANGELOG ya tiene la versión ${version}`);
  return md.replace(/^## \[Unreleased\]/m, `## [Unreleased]${nl}${nl}## [${version}] — ${date}`);
}

/// Comprueba que todas las versiones coinciden con la etiqueta. Devuelve la
/// lista de discrepancias (vacía = todo bien).
export function versionMismatches(contents, version) {
  const out = [];
  for (const [file, read] of Object.entries(readers)) {
    let found;
    try {
      found = read(contents[file]);
    } catch {
      found = null;
    }
    if (found !== version) out.push(`${file}: ${found ?? "(no encontrada)"} ≠ ${version}`);
  }
  return out;
}
