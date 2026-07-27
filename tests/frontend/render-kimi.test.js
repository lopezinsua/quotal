// Tests de la sección Kimi en render.js — visibilidad según `configured` y
// pintado de las dos ventanas (5 h y semanal) con el contrato del backend.
// Como dom.js cachea los nodos al evaluarse, cada test monta el HTML real del
// widget ANTES de importar los módulos (resetModules + import dinámico).
import { describe, it, expect, beforeEach, vi } from "vitest";
import { readFileSync } from "node:fs";

// Carga el index.html real en el documento (solo el contenido de <body>).
// Ruta relativa al cwd: vitest corre desde la raíz del proyecto.
function loadDom() {
  const html = readFileSync("src/index.html", "utf8");
  document.body.innerHTML = html.match(/<body>([\s\S]*)<\/body>/)[1];
}

async function setup() {
  vi.resetModules();
  localStorage.clear();
  loadDom();
  const { el } = await import("../../src/dom.js");
  const { render } = await import("../../src/render.js");
  return { el, render };
}

// Payload mínimo viable: el plan de Claude va disponible y `kimi` varía por test.
const payload = (kimi) => ({
  active: { source: "online" },
  plan: {
    name: "Pro",
    available: true,
    session_percent: 10,
    session_severity: "normal",
    session_resets_at: null,
    weekly_percent: 5,
    weekly_severity: "normal",
    weekly_resets_at: null,
    fetched_at: new Date().toISOString(),
    source: "online",
  },
  kimi,
});

const kimiFull = {
  configured: true,
  available: true,
  error: null,
  membership: "LEVEL_INTERMEDIATE",
  session_percent: 14,
  session_resets_at: new Date(Date.now() + 3 * 3600e3).toISOString(),
  session_severity: "normal",
  weekly_percent: 3,
  weekly_resets_at: new Date(Date.now() + 7 * 86400e3).toISOString(),
  weekly_severity: "normal",
  fetched_at: new Date().toISOString(),
  source: "online",
};

describe("render · sección Kimi", () => {
  beforeEach(() => localStorage.clear());

  it("muestra la sección y pinta ambas ventanas con el payload completo", async () => {
    const { el, render } = await setup();
    render(payload(kimiFull));

    expect(el.kimiSection.classList.contains("hidden")).toBe(false);
    expect(el.kimiSessionPct.textContent).toBe("14% used");
    expect(el.kimiWeeklyPct.textContent).toBe("3% used");
    // Anchos de barra según el % (jsdom normaliza "14.0%" -> "14%").
    expect(el.kimiSessionFill.style.width).toBe("14%");
    expect(el.kimiWeeklyFill.style.width).toBe("3%");
    expect(el.kimiSessionReset.textContent).not.toBe("—");
    expect(el.kimiWeeklyReset.textContent).not.toBe("—");
  });

  it("oculta la sección entera cuando Kimi no está configurado", async () => {
    const { el, render } = await setup();
    render(
      payload({
        configured: false,
        available: false,
        error: null,
        session_percent: null,
        weekly_percent: null,
        source: "none",
        fetched_at: new Date().toISOString(),
      }),
    );
    expect(el.kimiSection.classList.contains("hidden")).toBe(true);
  });

  it("oculta la sección si el payload no trae campo kimi", async () => {
    const { el, render } = await setup();
    render(payload(undefined));
    expect(el.kimiSection.classList.contains("hidden")).toBe(true);
  });

  it("con available:false (fallo de red) las barras caen a «—»", async () => {
    const { el, render } = await setup();
    render(
      payload({
        ...kimiFull,
        available: false,
        error: "timeout",
        session_percent: null,
        session_resets_at: null,
        weekly_percent: null,
        weekly_resets_at: null,
      }),
    );

    expect(el.kimiSection.classList.contains("hidden")).toBe(false);
    expect(el.kimiSessionPct.textContent).toBe("—");
    expect(el.kimiWeeklyPct.textContent).toBe("—");
    expect(el.kimiSessionFill.style.width).toBe("0%");
    expect(el.kimiWeeklyFill.style.width).toBe("0%");
  });
});
