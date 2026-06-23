// ESLint (flat config) para el frontend del widget. Su razón de ser principal es
// `no-undef`: un refactor que dividió `window.js` dejó imports incompletos
// (`BASE_H` usado sin importar) que rompían el layout en runtime sin que nada lo
// detectara. Esta regla lo convierte en un fallo de CI, no en un bug de producción.
import js from "@eslint/js";
import globals from "globals";

export default [
  js.configs.recommended,
  {
    files: ["src/**/*.js"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      // El frontend corre dentro del webview de Tauri: entorno de navegador
      // (window, document, fetch, localStorage, navigator, performance…).
      globals: { ...globals.browser },
    },
    rules: {
      "no-undef": "error",
      // No tratar como error las variables de error capturadas y no usadas
      // (`catch (e) { /* ignore */ }`), un patrón deliberado aquí.
      "no-unused-vars": ["error", { caughtErrors: "none" }],
    },
  },
];
