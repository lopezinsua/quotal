// Tests de prefs.js — vaciado de guardados diferidos. Algunos ajustes se
// persisten con debounce (tamaño/posición); `flushPrefs` debe vaciar ese trabajo
// pendiente y persistir a localStorage de una sola vez, para no perder el último
// ajuste del usuario si la app se cierra antes de que salte el temporizador.
import { describe, it, expect, beforeEach, vi } from "vitest";

const { onFlushPrefs, flushPrefs, prefs, PREFS_KEY } = await import("../../src/prefs.js");

describe("flushPrefs", () => {
  beforeEach(() => localStorage.clear());

  it("ejecuta los flushers registrados y persiste una sola vez a localStorage", () => {
    let ran = 0;
    const off = onFlushPrefs(() => {
      ran++;
      prefs.fullSize = { w: 300, h: 200 };
    });
    flushPrefs();
    expect(ran).toBe(1);
    const saved = JSON.parse(localStorage.getItem(PREFS_KEY));
    expect(saved.fullSize).toEqual({ w: 300, h: 200 });
    off();
  });

  it("des-registra un flusher cuando se invoca su retorno", () => {
    let ran = 0;
    const off = onFlushPrefs(() => ran++);
    off();
    flushPrefs();
    expect(ran).toBe(0);
  });

  it("un flusher que lanza no impide el guardado del resto", () => {
    const off1 = onFlushPrefs(() => {
      throw new Error("boom");
    });
    let ran = 0;
    const off2 = onFlushPrefs(() => ran++);
    expect(() => flushPrefs()).not.toThrow();
    expect(ran).toBe(1);
    expect(localStorage.getItem(PREFS_KEY)).not.toBeNull();
    off1();
    off2();
  });
});

// Un `widget-prefs` corrupto NO puede tumbar el arranque: prefs.js se evalúa al
// importarse y, si lanzara, main.js no llegaría a mostrar la ventana (nace oculta).
describe("carga de preferencias guardadas", () => {
  const fresh = async (stored) => {
    vi.resetModules();
    localStorage.clear();
    if (stored !== undefined) localStorage.setItem("widget-prefs", stored);
    return import("../../src/prefs.js");
  };

  it("con JSON corrupto arranca con los valores de fábrica", async () => {
    const { prefs } = await fresh("{roto");
    expect(prefs.onTop).toBe(true);
    expect(prefs.collapsed).toBe(false);
  });

  it("ignora un valor guardado que no es un objeto", async () => {
    const { prefs } = await fresh("[1,2,3]");
    expect(prefs.pillStyle).toBe("bar");
    expect(prefs[0]).toBeUndefined();
  });

  it("mezcla lo guardado sobre los valores de fábrica", async () => {
    const { prefs } = await fresh(JSON.stringify({ collapsed: true, theme: "light" }));
    expect(prefs.collapsed).toBe(true);
    expect(prefs.theme).toBe("light");
    expect(prefs.onTop).toBe(true);
  });
});
