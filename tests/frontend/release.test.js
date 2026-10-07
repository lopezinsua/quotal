// Tests de release.js — novedades de versión, errores legibles del updater y
// cuándo mostrar el aviso de "actualizado". Con la tabla i18n inglesa incrustada.
import { describe, it, expect } from "vitest";
import { notesItems, updateErrorText, afterUpdateNotice } from "../../src/release.js";

// Forma real de una sección del CHANGELOG (Keep a Changelog).
const NOTES = `### Added
- **Pace marker on every usage bar.** A thin tick shows how much of the window
  has elapsed.
- **"Limit in ~X" projection.** See [the docs](https://example.com/x).

### Fixed
- Linux: no more false \`ldconfig\` warning.
`;

describe("notesItems", () => {
  it("se queda con los puntos, une las continuaciones y quita la sintaxis", () => {
    expect(notesItems(NOTES)).toEqual([
      "Pace marker on every usage bar. A thin tick shows how much of the window has elapsed.",
      '"Limit in ~X" projection. See the docs.',
      "Linux: no more false ldconfig warning.",
    ]);
  });

  it("recorta a `max` puntos indicando cuántos quedan fuera", () => {
    const md = Array.from({ length: 12 }, (_, i) => `- cambio ${i + 1}`).join("\n");
    const items = notesItems(md, 5);
    expect(items).toHaveLength(5);
    expect(items[3]).toBe("cambio 4");
    expect(items[4]).toBe("…and 8 more changes");
  });

  it("sin notas → lista vacía", () => {
    expect(notesItems(null)).toEqual([]);
    expect(notesItems("")).toEqual([]);
    expect(notesItems("## Solo un título")).toEqual([]);
  });

  it("nunca devuelve marcado HTML interpretable (solo texto)", () => {
    // Se pinta con textContent; aquí basta con comprobar que el texto pasa tal cual.
    expect(notesItems("- <img src=x onerror=alert(1)>")).toEqual(["<img src=x onerror=alert(1)>"]);
  });
});

describe("updateErrorText", () => {
  it("traduce los fallos de red y de firma", () => {
    expect(updateErrorText("error sending request for url (https://github.com/…)")).toMatch(
      /Couldn't reach GitHub/,
    );
    expect(updateErrorText("Network request timed out")).toMatch(/Couldn't reach GitHub/);
    expect(updateErrorText("signature verification failed")).toMatch(/signature/);
  });
  it("lo desconocido va dentro del mensaje genérico", () => {
    expect(updateErrorText("disk full")).toBe("Update failed: disk full");
  });
});

describe("afterUpdateNotice", () => {
  it("avisa al arrancar con una versión distinta a la última vista", () => {
    const n = afterUpdateNotice("0.3.4", "0.4.0", { version: "0.4.0", notes: "- x" });
    expect(n).toEqual({ show: true, notes: "- x" });
  });
  it("sin novedades guardadas (o de otra versión) avisa igualmente, sin notas", () => {
    expect(afterUpdateNotice("0.3.4", "0.4.0", null)).toEqual({ show: true, notes: null });
    expect(afterUpdateNotice("0.3.4", "0.4.0", { version: "0.3.9", notes: "- y" }).notes).toBeNull();
  });
  it("primera ejecución o misma versión → nada", () => {
    expect(afterUpdateNotice(undefined, "0.4.0", null).show).toBe(false);
    expect(afterUpdateNotice("0.4.0", "0.4.0", null).show).toBe(false);
  });
});
