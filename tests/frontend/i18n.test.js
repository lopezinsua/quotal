// Tests de i18n — paridad de claves entre idiomas. Al faltar una clave, `t()` cae
// al inglés EN SILENCIO: así se colaron 11 textos sin traducir en 9 idiomas. Este
// test convierte ese hueco en un fallo de CI.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

const { BASE, SUPPORTED } = await import("../../src/i18n.js");
// En jsdom `import.meta.url` no es file:, así que resolvemos desde la raíz (cwd de vitest).
const dir = resolve("src/locales") + "/";
const placeholders = (s) => (String(s).match(/\{\w+\}/g) || []).sort();

describe("locales", () => {
  const files = readdirSync(dir).filter((f) => f.endsWith(".json"));

  it("hay un fichero por cada idioma soportado (salvo el inglés incrustado)", () => {
    const codes = files.map((f) => f.replace(".json", "")).sort();
    expect(codes).toEqual(Object.keys(SUPPORTED).filter((c) => c !== "en").sort());
  });

  for (const f of files) {
    const table = JSON.parse(readFileSync(dir + f, "utf8"));

    it(`${f} traduce todas las claves del inglés`, () => {
      const missing = Object.keys(BASE).filter((k) => !(k in table));
      expect(missing).toEqual([]);
      const unknown = Object.keys(table).filter((k) => !(k in BASE));
      expect(unknown).toEqual([]);
    });

    it(`${f} conserva los marcadores {x} de cada texto`, () => {
      for (const [k, v] of Object.entries(table)) {
        expect([k, placeholders(v)]).toEqual([k, placeholders(BASE[k])]);
      }
    });
  }
});
