// Tests de anchor.js — anclaje de la ventana por la esquina más cercana. Probamos
// la validación de formato (normAnchor) y la conversión esquina→top-left
// (anchorToTopLeft) pasándole un monitor explícito para que NO toque la API de
// Tauri (que mockeamos igualmente, porque la cadena de imports la carga).
import { describe, it, expect, vi } from "vitest";

vi.mock("../../src/tauri.js", () => {
  class PhysicalPosition {
    constructor(x, y) {
      this.x = x;
      this.y = y;
    }
  }
  return {
    invoke: () => Promise.resolve(),
    win: { scaleFactor: () => Promise.resolve(1) },
    currentMonitor: () => Promise.resolve(null),
    availableMonitors: () => Promise.resolve([]),
    PhysicalPosition,
  };
});

const { normAnchor, anchorToTopLeft } = await import("../../src/anchor.js");

const MON = { position: { x: 0, y: 0 }, size: { width: 1920, height: 1080 } };

describe("normAnchor", () => {
  it("acepta un ancla bien formada", () => {
    const a = { right: true, bottom: false, x: 100, y: 40 };
    expect(normAnchor(a)).toBe(a);
  });
  it("rechaza formatos antiguos o inválidos", () => {
    expect(normAnchor(null)).toBeNull();
    expect(normAnchor({ x: 1, y: 2 })).toBeNull(); // sin right/bottom boolean
    expect(normAnchor({ right: true, bottom: false, x: NaN, y: 0 })).toBeNull();
    expect(normAnchor({ right: "yes", bottom: false, x: 1, y: 2 })).toBeNull();
  });
});

describe("anchorToTopLeft", () => {
  it("esquina superior-derecha: crece hacia el interior (x = ancla - ancho)", async () => {
    const tl = await anchorToTopLeft({ right: true, bottom: false, x: 1000, y: 40 }, 248, 268, MON);
    expect(tl).toEqual({ x: 1000 - 248, y: 40 });
  });

  it("esquina superior-izquierda: el top-left coincide con el ancla", async () => {
    const tl = await anchorToTopLeft({ right: false, bottom: false, x: 12, y: 12 }, 248, 268, MON);
    expect(tl).toEqual({ x: 12, y: 12 });
  });

  it("esquina inferior-derecha: descuenta ancho y alto", async () => {
    const tl = await anchorToTopLeft(
      { right: true, bottom: true, x: 1920, y: 1080 },
      248,
      268,
      MON,
    );
    expect(tl).toEqual({ x: 1920 - 248, y: 1080 - 268 });
  });

  it("acota dentro del monitor (no se sale por el borde)", async () => {
    // Ancla pegada al borde izquierdo/superior con coordenada negativa → se clava
    // en el origen del monitor.
    const tl = await anchorToTopLeft({ right: false, bottom: false, x: -50, y: -50 }, 248, 268, MON);
    expect(tl).toEqual({ x: 0, y: 0 });
  });
});
