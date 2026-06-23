import { defineConfig } from "vitest/config";

// Tests del frontend (sin framework, módulos ES nativos). Entorno jsdom porque
// varios módulos tocan `window`/`localStorage` al cargarse (prefs, history) o
// importan la API de Tauri inyectada en `window` (geometry, anchor) — esta última
// se mockea por test. No hay E2E todavía: solo lógica pura y de presentación.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["tests/frontend/**/*.test.js"],
  },
});
