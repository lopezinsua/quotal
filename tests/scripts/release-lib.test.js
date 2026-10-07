// Tests de scripts/release-lib.mjs — la guardia que impide publicar una release
// cuya versión no cuadra con la etiqueta (el `latest.json` anunciaría la versión
// vieja y ninguna app instalada se actualizaría) y la extracción de notas.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  readers,
  writers,
  VERSION_FILES,
  versionMismatches,
  changelogSection,
  releaseChangelog,
} from "../../scripts/release-lib.mjs";

// Los ficheros REALES del repo: si su formato cambia, el test lo detecta.
const real = Object.fromEntries(VERSION_FILES.map((f) => [f, readFileSync(resolve(f), "utf8")]));
const current = readers["src-tauri/tauri.conf.json"](real["src-tauri/tauri.conf.json"]);

describe("versiones del repo", () => {
  it("los cinco ficheros declaran la misma versión", () => {
    expect(current).toMatch(/^\d+\.\d+\.\d+/);
    expect(versionMismatches(real, current)).toEqual([]);
  });

  it("subir la versión cambia los cinco y nada más que la versión", () => {
    for (const f of VERSION_FILES) {
      const bumped = writers[f](real[f], "9.8.7");
      expect(readers[f](bumped), f).toBe("9.8.7");
      // Solo cambian las líneas de versión (1, o 2 en package-lock).
      const a = real[f].split(/\r?\n/);
      const b = bumped.split(/\r?\n/);
      expect(b).toHaveLength(a.length);
      const changed = a.filter((l, i) => l !== b[i]).length;
      expect(changed, f).toBe(f === "package-lock.json" ? 2 : 1);
    }
  });

  it("no toca la versión de otras crates del Cargo.lock", () => {
    const lock = real["src-tauri/Cargo.lock"];
    const bumped = writers["src-tauri/Cargo.lock"](lock, "9.8.7");
    expect(bumped.match(/version = "9\.8\.7"/g)).toHaveLength(1);
  });

  it("detecta una etiqueta que no cuadra", () => {
    const errs = versionMismatches(real, "99.0.0");
    expect(errs).toHaveLength(VERSION_FILES.length);
    const half = { ...real, "package.json": writers["package.json"](real["package.json"], "99.0.0") };
    expect(versionMismatches(half, "99.0.0")).toHaveLength(VERSION_FILES.length - 1);
  });
});

const CL = `# Changelog

## [Unreleased]

### Added
- Cosa nueva.

## [0.3.4] — 2026-07-10

### Fixed
- Arreglo.

## [0.3.3] — 2026-07-01
- Otra.

[0.3.4]: https://example.com
`;

describe("changelog", () => {
  it("extrae el cuerpo de una sección", () => {
    expect(changelogSection(CL, "0.3.4")).toBe("### Fixed\n- Arreglo.");
    expect(changelogSection(CL, "0.3.3")).toBe("- Otra.");
    expect(changelogSection(CL, "Unreleased")).toBe("### Added\n- Cosa nueva.");
    expect(changelogSection(CL, "1.0.0")).toBeNull();
  });

  it("convierte Unreleased en la versión nueva y deja una vacía encima", () => {
    const out = releaseChangelog(CL, "0.4.0", "2026-10-08");
    expect(out).toContain("## [Unreleased]\n\n## [0.4.0] — 2026-10-08\n\n### Added\n- Cosa nueva.");
    expect(changelogSection(out, "0.4.0")).toBe("### Added\n- Cosa nueva.");
    expect(changelogSection(out, "Unreleased")).toBeNull();
  });

  it("se niega si no hay nada pendiente o la versión ya existe", () => {
    const vacio = CL.replace("### Added\n- Cosa nueva.\n", "");
    expect(() => releaseChangelog(vacio, "0.4.0", "x")).toThrow(/vacía/);
    expect(() => releaseChangelog(CL, "0.3.4", "x")).toThrow(/ya tiene/);
  });

  it("el CHANGELOG real tiene sección para la versión actual", () => {
    expect(changelogSection(readFileSync(resolve("CHANGELOG.md"), "utf8"), current)).toBeTruthy();
  });
});
