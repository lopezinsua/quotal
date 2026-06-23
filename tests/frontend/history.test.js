// Tests de history.js — serie temporal del % de sesión (sparkline). Tiene estado
// a nivel de módulo y persiste en localStorage, así que cada test reimporta el
// módulo en limpio (`resetModules`) tras vaciar localStorage para aislarse.
import { describe, it, expect, beforeEach, vi } from "vitest";

async function freshHistory() {
  localStorage.clear();
  vi.resetModules();
  return import("../../src/history.js");
}

describe("history.pushSample / series", () => {
  beforeEach(() => localStorage.clear());

  it("ignora valores null y no crea muestra", async () => {
    const { pushSample, series } = await freshHistory();
    expect(pushSample(null, "a")).toBe(false);
    expect(series()).toEqual([]);
  });

  it("añade muestras nuevas y deduplica por marca de obtención", async () => {
    const { pushSample, series } = await freshHistory();
    expect(pushSample(50, "a")).toBe(true);
    expect(pushSample(50, "a")).toBe(false); // misma marca: re-pintado, no duplica
    expect(pushSample(60, "b")).toBe(true);
    expect(series()).toEqual([50, 60]);
  });

  it("satura el valor al rango 0..100", async () => {
    const { pushSample, series } = await freshHistory();
    pushSample(150, "a");
    pushSample(-20, "b");
    expect(series()).toEqual([100, 0]);
  });

  it("persiste en localStorage y se recarga al reimportar", async () => {
    const { pushSample } = await freshHistory();
    pushSample(42, "x");
    vi.resetModules();
    const { series } = await import("../../src/history.js");
    expect(series()).toEqual([42]); // recuperado del almacenamiento
  });

  it("recorta a un máximo de 40 muestras", async () => {
    const { pushSample, series } = await freshHistory();
    for (let i = 0; i < 50; i++) pushSample(i, `s${i}`);
    const s = series();
    expect(s.length).toBe(40);
    expect(s[s.length - 1]).toBe(49); // conserva las más recientes
    expect(s[0]).toBe(10); // 50 - 40
  });
});
