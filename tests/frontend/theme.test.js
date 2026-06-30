// Tests de theme.js — aplicación de tema y acento como clases CSS sobre la
// tarjeta. La convención clave: el valor por defecto (dark / default) NO añade
// clase (lo da el CSS base); el resto añade theme-* / accent-*. Y siempre se
// limpia la clase previa antes de aplicar la nueva (no se acumulan).
import { describe, it, expect, beforeEach } from "vitest";
import {
  applyTheme,
  applyAccent,
  theme,
  accent,
  THEMES,
  ACCENTS,
} from "../../src/theme.js";

let card;
beforeEach(() => {
  card = document.createElement("div");
  card.className = "card";
});

describe("theme()/accent() saneado", () => {
  it("cae al default ante valores inválidos o nulos", () => {
    expect(theme("nope")).toBe("dark");
    expect(theme(undefined)).toBe("dark");
    expect(theme("light")).toBe("light");
    expect(accent("nope")).toBe("default");
    expect(accent(null)).toBe("default");
    expect(accent("violet")).toBe("violet");
  });
});

describe("applyTheme", () => {
  it("el tema oscuro (default) no añade clase", () => {
    applyTheme(card, "dark");
    expect(card.classList.contains("theme-dark")).toBe(false);
    expect(card.classList.contains("theme-light")).toBe(false);
  });

  it("el tema claro añade theme-light", () => {
    applyTheme(card, "light");
    expect(card.classList.contains("theme-light")).toBe(true);
  });

  it("cambiar de claro a oscuro limpia theme-light", () => {
    applyTheme(card, "light");
    applyTheme(card, "dark");
    expect(card.classList.contains("theme-light")).toBe(false);
    // No quedan clases theme-* huérfanas.
    expect(THEMES.some((t) => card.classList.contains(`theme-${t}`))).toBe(false);
  });

  it("un valor inválido se trata como el default (sin clase)", () => {
    applyTheme(card, "banana");
    expect(THEMES.some((t) => card.classList.contains(`theme-${t}`))).toBe(false);
  });
});

describe("applyAccent", () => {
  it("'default' no añade clase", () => {
    applyAccent(card, "default");
    expect(ACCENTS.some((a) => card.classList.contains(`accent-${a}`))).toBe(false);
  });

  it("un acento concreto añade accent-<color>", () => {
    applyAccent(card, "blue");
    expect(card.classList.contains("accent-blue")).toBe(true);
  });

  it("cambiar de acento reemplaza la clase (no acumula)", () => {
    applyAccent(card, "blue");
    applyAccent(card, "violet");
    expect(card.classList.contains("accent-blue")).toBe(false);
    expect(card.classList.contains("accent-violet")).toBe(true);
    expect(
      ACCENTS.filter((a) => card.classList.contains(`accent-${a}`)),
    ).toHaveLength(1);
  });

  it("volver a 'default' limpia cualquier accent-*", () => {
    applyAccent(card, "amber");
    applyAccent(card, "default");
    expect(ACCENTS.some((a) => card.classList.contains(`accent-${a}`))).toBe(false);
  });
});

describe("tema y acento son independientes", () => {
  it("conviven theme-light y accent-green sin pisarse", () => {
    applyTheme(card, "light");
    applyAccent(card, "green");
    expect(card.classList.contains("theme-light")).toBe(true);
    expect(card.classList.contains("accent-green")).toBe(true);
    // La clase base se conserva.
    expect(card.classList.contains("card")).toBe(true);
  });
});
