// Test de HUMO del arranque: carga el `index.html` real y `main.js` (con TODOS sus
// módulos) sobre una API de Tauri simulada. Los tests unitarios no ven fallos de
// cableado en runtime —un import que falta, un id de nodo renombrado, un comando
// IPC mal escrito—, que dejan el widget en blanco o sin mostrarse. Aquí se ve.
import { describe, it, expect, vi, beforeAll } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const calls = [];
const listeners = {};

const PAYLOAD = {
  active: { source: "hook", tokens_used: 84000, tokens_limit: 200000, percent_used: 42 },
  plan: {
    name: "Max",
    available: true,
    session_percent: 37,
    session_resets_at: new Date(Date.now() + 2 * 3.6e6).toISOString(),
    weekly_percent: 12,
    weekly_resets_at: new Date(Date.now() + 3 * 864e5).toISOString(),
    fetched_at: new Date().toISOString(),
    source: "online",
  },
  schema_warning: null,
  claude_code_version: "2.1.200",
};

// Respuestas del backend simulado. Los toggles de Claude Code fallan con los
// códigos estables que devuelve el backend real.
function invoke(cmd, args) {
  calls.push({ cmd, args });
  switch (cmd) {
    case "get_metrics":
      return Promise.resolve(PAYLOAD);
    case "get_config":
      return Promise.resolve({ version: "9.9.9" });
    case "check_system_deps":
      return Promise.resolve({ missing: [] });
    case "autostart_status":
    case "shutdown_status":
    case "statusline_status":
    case "read_only_status":
    case "primary_button_down":
      return Promise.resolve(false);
    case "install_statusline_bridge":
      return Promise.reject("node_missing: Node.js no está en el PATH");
    case "install_autostart":
      return Promise.reject("settings_invalid: settings.json no es JSON válido");
    default:
      return Promise.resolve(null);
  }
}

class LogicalSize {
  constructor(w, h) {
    Object.assign(this, { width: w, height: h });
  }
}
class PhysicalPosition {
  constructor(x, y) {
    Object.assign(this, { x, y });
  }
}

const MON = {
  position: { x: 0, y: 0 },
  size: { width: 1920, height: 1080 },
  workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1032 } },
  scaleFactor: 1,
};

const win = {
  shown: 0,
  show() {
    this.shown++;
    return Promise.resolve();
  },
  setAlwaysOnTop: () => Promise.resolve(),
  setMinSize: () => Promise.resolve(),
  setMaxSize: () => Promise.resolve(),
  setSize: () => Promise.resolve(),
  setPosition: () => Promise.resolve(),
  scaleFactor: () => Promise.resolve(1),
  outerPosition: () => Promise.resolve({ x: 1660, y: 12 }),
  outerSize: () => Promise.resolve({ width: 248, height: 268 }),
  onMoved: () => Promise.resolve(() => {}),
  onResized: () => Promise.resolve(() => {}),
  startDragging: () => Promise.resolve(),
  startResizeDragging: () => Promise.resolve(),
};

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeAll(async () => {
  const html = readFileSync(resolve("src/index.html"), "utf8");
  document.documentElement.innerHTML = html.replace(/<script[\s\S]*?<\/script>/g, "");
  window.__TAURI__ = {
    core: { invoke },
    event: {
      listen: (name, cb) => {
        listeners[name] = cb;
        return Promise.resolve(() => {});
      },
    },
    window: {
      getCurrentWindow: () => win,
      currentMonitor: () => Promise.resolve(MON),
      primaryMonitor: () => Promise.resolve(MON),
      availableMonitors: () => Promise.resolve([MON]),
    },
    webview: { getCurrentWebview: () => ({ setZoom: () => Promise.resolve() }) },
    dpi: { LogicalSize, PhysicalPosition },
    notification: {},
  };
  localStorage.clear();
  // Historial de la sesión subiendo ~1 %/min durante media hora: con el 37 % actual
  // y el reinicio dentro de 2 h, la proyección debe avisar del límite (~1 h).
  const hist = Array.from({ length: 31 }, (_, i) => ({
    t: new Date(Date.now() - (31 - i) * 60_000).toISOString(),
    v: 6 + i,
  }));
  localStorage.setItem("widget-usage-history", JSON.stringify(hist));
  // Simula el arranque justo DESPUÉS de actualizar desde la 0.0.1 a la 9.9.9 (la
  // versión que devuelve get_config), con las novedades guardadas antes de instalar.
  localStorage.setItem(
    "widget-prefs",
    JSON.stringify({ lastVersion: "0.0.1", pendingUpdate: { version: "9.9.9", notes: "- Nuevo X" } }),
  );
  await import("../../src/main.js");
  // El arranque es una cadena de promesas (prefs → layout → posición → show):
  // esperamos a su ÚLTIMO paso en vez de contar ticks, que varía entre versiones.
  await vi.waitFor(() => expect(win.shown).toBeGreaterThan(0), { timeout: 3000 });
  await flush();
});

describe("arranque completo del frontend", () => {
  it("revela la ventana (nace oculta) y pinta el primer payload", () => {
    expect(win.shown).toBeGreaterThan(0);
    const card = document.getElementById("card");
    expect(card.classList.contains("loading")).toBe(false);
    expect(document.getElementById("plan-name").textContent).toBe("Max");
    expect(document.getElementById("session-pct").textContent).toBe("37% used");
    expect(document.getElementById("weekly-pct").textContent).toBe("12% used");
    expect(document.getElementById("context-label").textContent).toBe("Context · 200k");
  });

  it("marca el ritmo en las barras y proyecta el límite de la sesión", () => {
    const sessMark = document.querySelector("#session-block .pace-mark");
    const weekMark = document.querySelector("#weekly-block .pace-mark");
    expect(sessMark).not.toBeNull();
    expect(weekMark).not.toBeNull();
    // Sesión de 5 h que se reinicia dentro de 2 h → han pasado 3 h (60 %).
    expect(parseFloat(sessMark.style.left)).toBeCloseTo(60, 0);
    expect(sessMark.classList.contains("over")).toBe(false); // 37 % < 60 %
    const eta = document.getElementById("session-eta");
    expect(eta.classList.contains("hidden")).toBe(false);
    expect(eta.textContent).toMatch(/^Limit ~1h \d+m$/);
  });

  it("se suscribe a las actualizaciones del backend y repinta", async () => {
    expect(typeof listeners["usage://metrics-updated"]).toBe("function");
    listeners["usage://metrics-updated"]({
      payload: { ...PAYLOAD, plan: { ...PAYLOAD.plan, session_percent: 91 } },
    });
    await flush();
    expect(document.getElementById("card").classList.contains("sev-critical")).toBe(true);
  });

  it("envía a la bandeja sus textos en el idioma activo", () => {
    const sent = calls.filter((c) => c.cmd === "set_tray_labels");
    expect(sent.length).toBeGreaterThan(0);
    expect(sent.at(-1).args.labels).toMatchObject({ show: "Show widget", quit: "Quit" });
  });

  it("un toggle de Claude Code que falla se revierte y EXPLICA el motivo", async () => {
    const box = document.getElementById("opt-statusline");
    const err = document.getElementById("hooks-error");
    box.checked = true;
    box.dispatchEvent(new Event("change"));
    await flush();
    expect(box.checked).toBe(false);
    expect(err.classList.contains("hidden")).toBe(false);
    expect(err.textContent).toMatch(/Node\.js/);

    const auto = document.getElementById("opt-autostart");
    auto.checked = true;
    auto.dispatchEvent(new Event("change"));
    await flush();
    expect(auto.checked).toBe(false);
    expect(err.textContent).toMatch(/syntax error/);
  });

  it("tras actualizar avisa una vez con las novedades guardadas", () => {
    const banner = document.getElementById("updated-banner");
    expect(banner.classList.contains("hidden")).toBe(false);
    expect(document.getElementById("updated-text").textContent).toBe("Updated to v9.9.9");
    expect(document.getElementById("updated-notes").textContent).toBe("Nuevo X");
    const saved = JSON.parse(localStorage.getItem("widget-prefs"));
    expect(saved.lastVersion).toBe("9.9.9");
    expect(saved.pendingUpdate).toBeNull();
    // El aviso ocupa el sitio de la cabecera del plan (alto fijo de la tarjeta).
    expect(document.getElementById("card").classList.contains("has-notice")).toBe(true);
    document.getElementById("updated-dismiss").click();
    expect(banner.classList.contains("hidden")).toBe(true);
    expect(document.getElementById("card").classList.contains("has-notice")).toBe(false);
  });

  it("avisa de una versión nueva con novedades y muestra el progreso", async () => {
    listeners["update://available"]({
      payload: { available: true, version: "10.0.0", notes: "### Added\n- Cosa A\n- Cosa B" },
    });
    await flush();
    expect(document.getElementById("update-banner").classList.contains("hidden")).toBe(false);
    expect(document.getElementById("card").classList.contains("has-notice")).toBe(true);
    const text = document.getElementById("update-text");
    expect(text.textContent).toBe("v10.0.0 available");
    expect(text.title).toBe("Update available: v10.0.0");
    // El ⌄ despliega las novedades y "No mostrar más".
    const toggle = document.getElementById("update-notes-toggle");
    const more = document.getElementById("update-more");
    expect(more.classList.contains("hidden")).toBe(true);
    toggle.click();
    expect(more.classList.contains("hidden")).toBe(false);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(document.getElementById("update-notes").children).toHaveLength(2);

    listeners["update://progress"]({ payload: { phase: "download", downloaded: 512, total: 2048 } });
    expect(text.textContent).toBe("Downloading… 25%");
    expect(document.getElementById("update-progress-fill").style.width).toBe("25%");
  });

  it("Descartar no vuelve a sacar la misma versión en la sesión", async () => {
    document.getElementById("update-dismiss").click();
    expect(document.getElementById("card").classList.contains("has-notice")).toBe(false);
    listeners["update://available"]({ payload: { available: true, version: "10.0.0" } });
    await flush();
    expect(document.getElementById("update-banner").classList.contains("hidden")).toBe(true);
  });

  it("muestra la versión instalada en ajustes", () => {
    expect(document.getElementById("upd-current").textContent).toContain("9.9.9");
  });
});
