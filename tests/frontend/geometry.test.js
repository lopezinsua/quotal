// Tests de geometry.js — matemática de escala/tamaño de la ventana. El módulo
// importa ./tauri.js (API inyectada en window por Tauri) y llama a win.scaleFactor()
// al cargarse, así que mockeamos ./tauri.js con stubs inertes.
import { describe, it, expect, vi } from "vitest";

vi.mock("../../src/tauri.js", () => {
  const win = { scaleFactor: () => Promise.resolve(1) };
  let mons = [];
  return {
    win,
    webview: { setZoom: () => Promise.resolve() },
    currentMonitor: () => Promise.resolve(null),
    primaryMonitor: () => Promise.resolve(null),
    availableMonitors: () => Promise.resolve(mons),
    // Solo para tests: fija lo que devuelve availableMonitors.
    __setMonitors: (m) => {
      mons = m;
    },
  };
});

const { __setMonitors } = await import("../../src/tauri.js");
const {
  BASE_W,
  BASE_H,
  MIN_SCALE,
  MAX_SCALE,
  FULL_MIN,
  SNAP_MARGIN,
  clamp,
  fitMaxScale,
  fullSizeFor,
  snapTopLeft,
  monitorFromPoint,
} = await import("../../src/geometry.js");

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

describe("snapTopLeft", () => {
  const AREA = { position: { x: 0, y: 0 }, size: { width: 1920, height: 1080 } };
  const SIZE = { width: 248, height: 268 };

  it("lejos de todo borde → no cambia", () => {
    expect(snapTopLeft({ x: 500, y: 400 }, SIZE, AREA, 1)).toEqual({ x: 500, y: 400 });
  });

  it("cerca del borde derecho → se pega con el margen estándar", () => {
    // Hueco a la derecha: 1920 - (1652 + 248) = 20 < 28 → x = 1920 - 248 - 12.
    const tl = snapTopLeft({ x: 1652, y: 400 }, SIZE, AREA, 1);
    expect(tl).toEqual({ x: 1920 - 248 - SNAP_MARGIN, y: 400 });
  });

  it("cerca de dos bordes (esquina) → se pega a ambos", () => {
    const tl = snapTopLeft({ x: 5, y: 1080 - 268 - 3 }, SIZE, AREA, 1);
    expect(tl).toEqual({ x: SNAP_MARGIN, y: 1080 - 268 - SNAP_MARGIN });
  });

  it("medio fuera de pantalla (hueco negativo) → la re-mete al borde", () => {
    const tl = snapTopLeft({ x: -80, y: 400 }, SIZE, AREA, 1);
    expect(tl).toEqual({ x: SNAP_MARGIN, y: 400 });
  });

  it("escala umbral y margen con el factor del monitor", () => {
    // A escala 2: umbral 56 y margen 24 físicos. Hueco de 40 (<56) → pega.
    const tl = snapTopLeft({ x: 40, y: 400 }, SIZE, AREA, 2);
    expect(tl).toEqual({ x: SNAP_MARGIN * 2, y: 400 });
    // El mismo hueco de 40 a escala 1 (umbral 28) NO pega.
    expect(snapTopLeft({ x: 40, y: 400 }, SIZE, AREA, 1)).toEqual({ x: 40, y: 400 });
  });

  it("respeta el área de trabajo desplazada (posición ≠ 0,0)", () => {
    const area = { position: { x: 1920, y: 0 }, size: { width: 1920, height: 1040 } };
    // Cerca del borde izquierdo del SEGUNDO monitor.
    const tl = snapTopLeft({ x: 1930, y: 400 }, SIZE, area, 1);
    expect(tl).toEqual({ x: 1920 + SNAP_MARGIN, y: 400 });
  });
});

describe("monitorFromPoint", () => {
  const MON1 = { position: { x: 0, y: 0 }, size: { width: 1920, height: 1080 }, scaleFactor: 1 };
  const MON2 = { position: { x: 1920, y: 0 }, size: { width: 2560, height: 1440 }, scaleFactor: 2 };

  it("devuelve el monitor que contiene el punto (multi-monitor)", async () => {
    __setMonitors([MON1, MON2]);
    expect(await monitorFromPoint(100, 100)).toBe(MON1);
    expect(await monitorFromPoint(2000, 100)).toBe(MON2);
    // La frontera pertenece al monitor de la derecha (x >= position.x).
    expect(await monitorFromPoint(1920, 0)).toBe(MON2);
  });

  it("punto fuera de todos los monitores → null", async () => {
    __setMonitors([MON1, MON2]);
    expect(await monitorFromPoint(-50, -50)).toBeNull();
    expect(await monitorFromPoint(9999, 9999)).toBeNull();
  });

  it("sin monitores disponibles → null", async () => {
    __setMonitors([]);
    expect(await monitorFromPoint(100, 100)).toBeNull();
  });
});
