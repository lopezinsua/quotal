// Tests de notify.js — detección de FLANCO del aviso de umbral. La lógica clave:
// no molestar en el primer dato (baseline), avisar una sola vez por ventana al
// cruzar el umbral, y re-armar cuando la ventana se resetea. El estado vive a
// nivel de módulo, así que aislamos cada test con `resetModules` y un mock fresco
// de `window.__TAURI__.notification` (capturado al evaluar el módulo).
import { describe, it, expect, beforeEach, vi } from "vitest";

// Prepara un entorno limpio: mock de notificación + prefs, y devuelve el módulo
// notify recién importado junto al array de avisos enviados.
async function setup({ enabled = true, threshold = 90 } = {}) {
  vi.resetModules();
  const sent = [];
  window.__TAURI__ = {
    notification: {
      isPermissionGranted: async () => true,
      requestPermission: async () => "granted",
      sendNotification: (n) => sent.push(n),
    },
  };
  localStorage.clear();
  const prefs = (await import("../../src/prefs.js")).prefs;
  prefs.notifyEnabled = enabled;
  prefs.notifyThreshold = threshold;
  const notify = await import("../../src/notify.js");
  return { notify, sent, prefs };
}

const plan = (session, weekly = 0, resets = "w1") => ({
  plan: {
    available: true,
    session_percent: session,
    weekly_percent: weekly,
    session_resets_at: resets,
    weekly_resets_at: "wk1",
  },
});

describe("notifyFromPayload", () => {
  beforeEach(() => localStorage.clear());

  it("no avisa en el primer dato aunque ya se supere el umbral (baseline)", async () => {
    const { notify, sent } = await setup();
    notify.notifyFromPayload(plan(95));
    expect(sent).toHaveLength(0);
  });

  it("avisa al CRUZAR el umbral tras el baseline", async () => {
    const { notify, sent } = await setup();
    notify.notifyFromPayload(plan(50)); // baseline
    notify.notifyFromPayload(plan(92)); // cruza
    expect(sent).toHaveLength(1);
    expect(sent[0].body).toContain("92%");
  });

  it("avisa una sola vez por ventana (sin spam)", async () => {
    const { notify, sent } = await setup();
    notify.notifyFromPayload(plan(50));
    notify.notifyFromPayload(plan(92));
    notify.notifyFromPayload(plan(95));
    notify.notifyFromPayload(plan(99));
    expect(sent).toHaveLength(1);
  });

  it("se re-arma cuando la ventana se resetea (cambia resets_at)", async () => {
    const { notify, sent } = await setup();
    notify.notifyFromPayload(plan(50, 0, "w1"));
    notify.notifyFromPayload(plan(92, 0, "w1")); // aviso 1
    notify.notifyFromPayload(plan(20, 0, "w2")); // nueva ventana, por debajo
    notify.notifyFromPayload(plan(93, 0, "w2")); // aviso 2
    expect(sent).toHaveLength(2);
  });

  it("desactivado no envía, pero mantiene el baseline (sin aviso retroactivo)", async () => {
    const { notify, sent, prefs } = await setup({ enabled: false });
    notify.notifyFromPayload(plan(50));
    notify.notifyFromPayload(plan(92)); // cruza, pero desactivado
    expect(sent).toHaveLength(0);
    // El usuario activa ahora; ya estaba por encima -> NO debe avisar retroactivamente.
    prefs.notifyEnabled = true;
    notify.notifyFromPayload(plan(94));
    expect(sent).toHaveLength(0);
  });

  it("avisa de sesión y de semana de forma independiente", async () => {
    const { notify, sent } = await setup();
    notify.notifyFromPayload(plan(50, 50));
    notify.notifyFromPayload(plan(91, 93));
    expect(sent).toHaveLength(2);
  });

  it("ignora planes no disponibles sin fijar baseline", async () => {
    const { notify, sent } = await setup();
    notify.notifyFromPayload({ plan: { available: false } });
    // Como no hubo baseline, el primer dato REAL por encima del umbral tampoco avisa.
    notify.notifyFromPayload(plan(95));
    expect(sent).toHaveLength(0);
  });
});
