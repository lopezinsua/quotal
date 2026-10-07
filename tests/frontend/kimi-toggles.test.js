// Tests de los toggles «Abrir/Cerrar con Kimi Code» en la pantalla de ajustes:
// el HTML los declara tras los de Claude con sus data-i18n, dom.js los cachea
// y los textos existen en el inglés base (i18n.js) y en el locale es.json.
import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";

function loadDom() {
  const html = readFileSync("src/index.html", "utf8");
  document.body.innerHTML = html.match(/<body>([\s\S]*)<\/body>/)[1];
}

describe("toggles Kimi en ajustes", () => {
  it("el HTML declara ambos toggles con su data-i18n", () => {
    loadDom();
    const autostart = document.getElementById("opt-kimi-autostart");
    const close = document.getElementById("opt-kimi-close");
    expect(autostart).not.toBeNull();
    expect(close).not.toBeNull();
    expect(
      autostart.closest("label").querySelector("[data-i18n]").dataset.i18n,
    ).toBe("opt_kimi_autostart");
    expect(
      close.closest("label").querySelector("[data-i18n]").dataset.i18n,
    ).toBe("opt_kimi_close");
  });

  it("dom.js cachea ambos nodos", async () => {
    vi.resetModules();
    loadDom();
    const { el } = await import("../../src/dom.js");
    expect(el.optKimiAutostart).toBe(document.getElementById("opt-kimi-autostart"));
    expect(el.optKimiClose).toBe(document.getElementById("opt-kimi-close"));
  });

  it("las claves existen en inglés base y en español", async () => {
    const { t } = await import("../../src/i18n.js");
    const es = JSON.parse(readFileSync("src/locales/es.json", "utf8"));
    // Sin idioma cargado, t() cae al inglés base incrustado.
    expect(t("opt_kimi_autostart")).toBe("Open with Kimi Code");
    expect(t("opt_kimi_close")).toBe("Close with Kimi Code");
    expect(es.opt_kimi_autostart).toBe("Abrir con Kimi Code");
    expect(es.opt_kimi_close).toBe("Cerrar con Kimi Code");
  });
});
