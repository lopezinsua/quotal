// Tests de prefs.js — vaciado de guardados diferidos. Algunos ajustes se
// persisten con debounce (tamaño/posición); `flushPrefs` debe vaciar ese trabajo
// pendiente y persistir a localStorage de una sola vez, para no perder el último
// ajuste del usuario si la app se cierra antes de que salte el temporizador.
import { describe, it, expect, beforeEach } from "vitest";

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
