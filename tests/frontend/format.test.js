// Tests de format.js — formateadores puros de presentación. Corren con la tabla
// i18n por defecto (inglés incrustado), así las cadenas son deterministas.
import { describe, it, expect } from "vitest";
import {
  worstSeverity,
  fmtWindow,
  fmtTokens,
  fmtResetIn,
  fmtResetAt,
  fmtFreshness,
  secsSince,
} from "../../src/format.js";

describe("worstSeverity", () => {
  it("deriva crítico/aviso del porcentaje cuando no hay severity", () => {
    expect(worstSeverity({ session_percent: 92 })).toBe("critical");
    expect(worstSeverity({ session_percent: 80 })).toBe("warning");
    expect(worstSeverity({ session_percent: 10 })).toBe("normal");
    expect(worstSeverity({})).toBe("normal");
  });
  it("respeta el campo severity de la API", () => {
    expect(worstSeverity({ session_severity: "critical", session_percent: 1 })).toBe("critical");
    expect(worstSeverity({ weekly_severity: "warning" })).toBe("warning");
  });
  it("toma la PEOR entre sesión y semana", () => {
    expect(worstSeverity({ session_percent: 10, weekly_percent: 95 })).toBe("critical");
  });
});

describe("fmtWindow", () => {
  it("compacta la ventana de contexto", () => {
    expect(fmtWindow(1_000_000)).toBe("1M");
    expect(fmtWindow(1_500_000)).toBe("1.5M");
    expect(fmtWindow(200_000)).toBe("200k");
    expect(fmtWindow(500)).toBe("500");
    expect(fmtWindow(null)).toBe("");
  });
});

describe("fmtTokens", () => {
  it("formatea con sufijos y maneja null", () => {
    expect(fmtTokens(null)).toBe("—");
    expect(fmtTokens(999)).toBe("999");
    expect(fmtTokens(1_234)).toBe("1.2k");
    expect(fmtTokens(2_000_000)).toBe("2.00M");
    expect(fmtTokens(3_000_000_000)).toBe("3.00B");
  });
});

describe("fmtResetIn", () => {
  it("sin fecha → guion; pasado → 'resets now'", () => {
    expect(fmtResetIn(null)).toBe("Resets —");
    expect(fmtResetIn(new Date(Date.now() - 1000).toISOString())).toBe("Resets now");
    // QUIRK documentado: una fecha INVÁLIDA cae en la rama isNaN(ms) y devuelve
    // "Resets now" (no el guion, a diferencia de fmtResetAt). Inofensivo porque la
    // entrada real siempre es un ISO del servidor o null; se fija aquí para que el
    // comportamiento quede explícito y no cambie sin querer.
    expect(fmtResetIn("no es fecha")).toBe("Resets now");
  });
  it("futuro < 1h usa minutos; ≥ 1h usa horas+minutos", () => {
    const inMin = new Date(Date.now() + 5 * 60_000 + 30_000).toISOString();
    expect(fmtResetIn(inMin)).toBe("Resets in 5m");
    const inHrs = new Date(Date.now() + 3 * 3.6e6 + 55 * 60_000 + 30_000).toISOString();
    expect(fmtResetIn(inHrs)).toBe("Resets in 3h 55m");
  });
});

describe("fmtResetAt", () => {
  it("sin fecha o inválida → guion", () => {
    expect(fmtResetAt(null)).toBe("Resets —");
    expect(fmtResetAt("xxx")).toBe("Resets —");
  });
  it("fecha válida → plantilla 'Resets {day}, {time}' rellenada", () => {
    const out = fmtResetAt(new Date("2026-06-25T10:00:00Z").toISOString());
    expect(out.startsWith("Resets ")).toBe(true);
    expect(out).toContain(", ");
    expect(out).not.toBe("Resets —");
  });
});

describe("fmtFreshness", () => {
  it("traduce franjas de antigüedad", () => {
    expect(fmtFreshness(null)).toBe("no data");
    expect(fmtFreshness(Number.MAX_SAFE_INTEGER)).toBe("no data");
    expect(fmtFreshness(30)).toBe("just now");
    expect(fmtFreshness(120)).toBe("2 min ago");
    expect(fmtFreshness(7200)).toBe("2h ago");
  });
});

describe("secsSince", () => {
  it("null/ inválido → null; pasado → segundos ≥ 0", () => {
    expect(secsSince(null)).toBeNull();
    expect(secsSince("nope")).toBeNull();
    const s = secsSince(new Date(Date.now() - 10_000).toISOString());
    expect(s).toBeGreaterThanOrEqual(9);
    expect(s).toBeLessThan(60);
  });
});
