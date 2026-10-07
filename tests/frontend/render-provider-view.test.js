// Tests de la vista de proveedor (prefs.providerView): qué bloques se ven en
// cada modo ("both" | "claude" | "kimi"), de dónde sale el % de la píldora y
// el aviso de "sin credenciales" de Kimi. Mismo montaje que render-kimi.test.js:
// HTML real del widget + import dinámico tras resetModules (dom.js cachea los
// nodos al evaluarse). prefs es singleton: cada test fija providerView
// EXPLÍCITAMENTE y el afterEach lo restaura a "both" (su default).
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
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
  const { prefs } = await import("../../src/prefs.js");
  return { el, render, prefs };
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

const kimiSinCreds = {
  configured: false,
  available: false,
  error: null,
  session_percent: null,
  weekly_percent: null,
  source: "none",
  fetched_at: new Date().toISOString(),
};

describe("render · vista de proveedor (providerView)", () => {
  beforeEach(() => localStorage.clear());
  afterEach(async () => {
    // prefs es singleton: restaura el default para no contaminar otros tests.
    const { prefs } = await import("../../src/prefs.js");
    prefs.providerView = "both";
    localStorage.clear();
  });

  it("modo 'kimi': oculta los bloques de Claude y muestra la sección Kimi", async () => {
    const { el, render, prefs } = await setup();
    prefs.providerView = "kimi";
    render(payload(kimiFull));

    expect(el.planHead.classList.contains("hidden")).toBe(true);
    expect(el.sessionBlock.classList.contains("hidden")).toBe(true);
    expect(el.weeklyBlock.classList.contains("hidden")).toBe(true);
    expect(el.contextLine.classList.contains("hidden")).toBe(true);
    expect(el.kimiSection.classList.contains("hidden")).toBe(false);
    expect(el.kimiSessionBlock.classList.contains("hidden")).toBe(false);
    expect(el.kimiWeeklyBlock.classList.contains("hidden")).toBe(false);
    expect(el.kimiNoCreds.classList.contains("hidden")).toBe(true);
  });

  it("modo 'claude': oculta la sección Kimi y mantiene los bloques de Claude", async () => {
    const { el, render, prefs } = await setup();
    prefs.providerView = "claude";
    render(payload(kimiFull));

    expect(el.kimiSection.classList.contains("hidden")).toBe(true);
    expect(el.planHead.classList.contains("hidden")).toBe(false);
    expect(el.sessionBlock.classList.contains("hidden")).toBe(false);
    expect(el.weeklyBlock.classList.contains("hidden")).toBe(false);
    expect(el.contextLine.classList.contains("hidden")).toBe(false);
  });

  it("modo 'kimi' sin credenciales: muestra #kimi-no-creds en lugar de las barras", async () => {
    const { el, render, prefs } = await setup();
    prefs.providerView = "kimi";
    render(payload(kimiSinCreds));

    expect(el.kimiSection.classList.contains("hidden")).toBe(false);
    expect(el.kimiNoCreds.classList.contains("hidden")).toBe(false);
    expect(el.kimiSessionBlock.classList.contains("hidden")).toBe(true);
    expect(el.kimiWeeklyBlock.classList.contains("hidden")).toBe(true);
  });

  it("modo 'kimi': la píldora se alimenta del % de la ventana de 5 h de Kimi", async () => {
    const { el, render, prefs } = await setup();
    prefs.providerView = "kimi";
    render(payload(kimiFull)); // plan 10%, kimi 14%

    expect(el.pillPct.textContent).toBe("14%");
  });

  it("modo 'both' (default): todo visible y la píldora sigue con el % de Claude", async () => {
    const { el, render, prefs } = await setup();
    prefs.providerView = "both";
    render(payload(kimiFull)); // plan 10%, kimi 14%

    expect(el.pillPct.textContent).toBe("10%");
    expect(el.planHead.classList.contains("hidden")).toBe(false);
    expect(el.sessionBlock.classList.contains("hidden")).toBe(false);
    expect(el.weeklyBlock.classList.contains("hidden")).toBe(false);
    expect(el.kimiSection.classList.contains("hidden")).toBe(false);
    expect(el.kimiNoCreds.classList.contains("hidden")).toBe(true);
  });

  it("modo 'kimi': sin credenciales la píldora cae a «—»", async () => {
    const { el, render, prefs } = await setup();
    prefs.providerView = "kimi";
    render(payload(kimiSinCreds));

    expect(el.pillPct.textContent).toBe("—");
  });
});
