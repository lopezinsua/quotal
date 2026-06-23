// Tests de geometry.js — matemática de escala/tamaño de la ventana. El módulo
// importa ./tauri.js (API inyectada en window por Tauri) y llama a win.scaleFactor()
// al cargarse, así que mockeamos ./tauri.js con stubs inertes.
import { describe, it, expect, vi } from "vitest";

vi.mock("../../src/tauri.js", () => {
  const win = { scaleFactor: () => Promise.resolve(1) };
  return {
    win,
    webview: { setZoom: () => Promise.resolve() },
    currentMonitor: () => Promise.resolve(null),
    primaryMonitor: () => Promise.resolve(null),
  };
});

const { BASE_W, BASE_H, MIN_SCALE, MAX_SCALE, FULL_MIN, clamp, fitMaxScale, fullSizeFor } =
  await import("../../src/geometry.js");

const monitor = (w, h, sf = 1) => ({ scaleFactor: sf, size: { width: w, height: h } });

describe("clamp", () => {
  it("acota al rango [a, b]", () => {
    expect(clamp(5, 0, 10)).toBe(5);
    expect(clamp(-1, 0, 10)).toBe(0);
    expect(clamp(99, 0, 10)).toBe(10);
  });
});

describe("constantes derivadas", () => {
  it("BASE = tamaño completo por defecto (248×268)", () => {
    expect(BASE_W).toBe(248);
    expect(BASE_H).toBe(268);
  });
  it("FULL_MIN = BASE × MIN_SCALE redondeado", () => {
    expect(FULL_MIN).toEqual({
      w: Math.round(BASE_W * MIN_SCALE),
      h: Math.round(BASE_H * MIN_SCALE),
    });
  });
});

describe("fitMaxScale", () => {
  it("sin monitor → MAX_SCALE", () => {
    expect(fitMaxScale(null)).toBe(MAX_SCALE);
  });
  it("monitor grande → topa en MAX_SCALE", () => {
    expect(fitMaxScale(monitor(1920, 1080))).toBe(MAX_SCALE);
  });
  it("monitor pequeño → escala que cabe (eje más restrictivo), nunca < MIN_SCALE", () => {
    // availH = 600 - 24 = 576; 576/268 ≈ 2.149 es el eje limitante.
    expect(fitMaxScale(monitor(600, 600))).toBeCloseTo(576 / BASE_H, 5);
    // Monitor diminuto: no baja de MIN_SCALE.
    expect(fitMaxScale(monitor(120, 120))).toBe(MIN_SCALE);
  });
});

describe("fullSizeFor", () => {
  it("con el tamaño por defecto y monitor amplio → escala 1 (248×268)", () => {
    const r = fullSizeFor(monitor(1920, 1080));
    expect(r.scale).toBe(1);
    expect(r.w).toBe(BASE_W);
    expect(r.h).toBe(BASE_H);
    expect(r.fit).toBe(MAX_SCALE);
  });
});
