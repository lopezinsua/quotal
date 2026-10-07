// Tests de pace.js — marca de ritmo y proyección al límite. Toda la aritmética
// recibe `now` explícito: deterministas, sin relojes falsos.
import { describe, it, expect } from "vitest";
import {
  windowElapsed,
  limitEta,
  SESSION_WINDOW_MS,
  WEEKLY_WINDOW_MS,
} from "../../src/pace.js";
import { fmtEta } from "../../src/format.js";

const NOW = Date.parse("2026-07-01T12:00:00Z");
const MIN = 60_000;
const iso = (ms) => new Date(ms).toISOString();
// Serie lineal: `n` muestras cada `every` ms terminando en `NOW`, subiendo `perMin` %/min.
const ramp = (n, every, last, perMin) =>
  Array.from({ length: n }, (_, i) => {
    const t = NOW - (n - 1 - i) * every;
    return { t, v: last - ((NOW - t) / MIN) * perMin };
  });

describe("windowElapsed", () => {
  it("fracción transcurrida de la ventana según su reinicio", () => {
    // Sesión de 5 h que se reinicia dentro de 2 h → han pasado 3 h (60 %).
    expect(windowElapsed(iso(NOW + 2 * 3.6e6), SESSION_WINDOW_MS, NOW)).toBeCloseTo(0.6);
    // Semana que se reinicia dentro de 7 días → recién empezada.
    expect(windowElapsed(iso(NOW + WEEKLY_WINDOW_MS), WEEKLY_WINDOW_MS, NOW)).toBe(0);
  });
  it("se acota a [0, 1] y tolera datos inválidos", () => {
    expect(windowElapsed(iso(NOW - MIN), SESSION_WINDOW_MS, NOW)).toBe(1);
    expect(windowElapsed(iso(NOW + 9 * 3.6e6), SESSION_WINDOW_MS, NOW)).toBe(0);
    expect(windowElapsed("no es fecha", SESSION_WINDOW_MS, NOW)).toBeNull();
    expect(windowElapsed(null, SESSION_WINDOW_MS, NOW)).toBeNull();
  });
});

describe("limitEta", () => {
  const reset3h = iso(NOW + 3 * 3.6e6);

  it("proyecta cuándo se llega al 100 % al ritmo reciente", () => {
    // 1 %/min, ahora en 40 % → faltan 60 min.
    const eta = limitEta(ramp(31, MIN, 40, 1), reset3h, SESSION_WINDOW_MS, NOW);
    expect(eta / MIN).toBeCloseTo(60, 5);
  });

  it("nada que avisar si el límite llegaría después del reinicio", () => {
    // 0,1 %/min desde 40 % → 600 min, pero se reinicia en 180.
    expect(limitEta(ramp(31, MIN, 40, 0.1), reset3h, SESSION_WINDOW_MS, NOW)).toBeNull();
  });

  it("nada que avisar con uso estable o a la baja", () => {
    expect(limitEta(ramp(31, MIN, 40, 0), reset3h, SESSION_WINDOW_MS, NOW)).toBeNull();
    expect(limitEta(ramp(31, MIN, 40, -0.5), reset3h, SESSION_WINDOW_MS, NOW)).toBeNull();
  });

  it("exige muestras suficientes y que cubran al menos 10 minutos", () => {
    expect(limitEta([{ t: NOW, v: 50 }], reset3h, SESSION_WINDOW_MS, NOW)).toBeNull();
    // Mucha pendiente pero en solo 5 min: una respuesta larga no es un ritmo.
    expect(limitEta(ramp(6, MIN, 50, 5), reset3h, SESSION_WINDOW_MS, NOW)).toBeNull();
  });

  it("ignora muestras de la ventana ANTERIOR", () => {
    // La ventana actual empezó hace 2 h (reinicio dentro de 3 h). Las muestras
    // anteriores a su inicio (con el 90 % de la ventana vieja) no cuentan.
    const start = NOW - 2 * 3.6e6;
    const old = [
      { t: start - 30 * MIN, v: 80 },
      { t: start - 10 * MIN, v: 95 },
    ];
    const fresh = [
      { t: NOW - 20 * MIN, v: 10 },
      { t: NOW, v: 10 },
    ];
    expect(limitEta([...old, ...fresh], reset3h, SESSION_WINDOW_MS, NOW)).toBeNull();
  });

  it("ya en el 100 % → 0; reinicio pasado o inválido → null", () => {
    expect(limitEta(ramp(20, MIN, 100, 1), reset3h, SESSION_WINDOW_MS, NOW)).toBe(0);
    expect(limitEta(ramp(20, MIN, 50, 1), iso(NOW - MIN), SESSION_WINDOW_MS, NOW)).toBeNull();
    expect(limitEta(ramp(20, MIN, 50, 1), "x", SESSION_WINDOW_MS, NOW)).toBeNull();
    expect(limitEta(null, reset3h, SESSION_WINDOW_MS, NOW)).toBeNull();
  });
});

describe("fmtEta", () => {
  it("formatea en horas+minutos o minutos, redondeando hacia arriba", () => {
    expect(fmtEta(80 * MIN)).toBe("Limit ~1h 20m");
    expect(fmtEta(25 * MIN + 1)).toBe("Limit ~26m");
    expect(fmtEta(0)).toBe("Limit ~1m");
  });
});
