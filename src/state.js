// state.js — Estado de UI compartido entre módulos (singleton mutable).
//
// Solo viven aquí los flags que cruzan más de un módulo. Los flags locales de un
// módulo (p. ej. `resizing` en window.js o `settingsOpen` en controls.js) se
// quedan en su módulo.

export const ui = {
  /// La píldora está "asomada" temporalmente por hover? (window.js + controls.js)
  peeking: false,
  /// ¿Se está ARRASTRANDO la ventana ahora mismo? Mientras dura, la vista se
  /// mantiene completa (como pineada) en vez de colapsar a la píldora a media
  /// maniobra. (window.js lo activa al iniciar el gesto de mover)
  moving: false,
  /// El puntero está dentro del widget? Decide, al terminar un arrastre, si se
  /// vuelve a la píldora (solo si quedó fuera). (controls.js lo mantiene)
  pointerInside: false,
  /// Hay un morph píldora<->completo en curso? Mientras dura, onResized (resize.js)
  /// no interviene: el tamaño lo conduce el bucle de Rust. (window.js lo gestiona)
  animating: false,
  /// Modo expandido de la pasada ANTERIOR de layout (null al arranque). Sirve para
  /// animar SOLO cuando cambia de modo. Compartido entre window.js (applyLayout) y
  /// drag.js (expandInPlace lo fija a true al desplegar para mover).
  prevExpanded: null,
  /// Último payload renderizado, para re-pintar sin esperar evento. (render/main/controls)
  lastPayload: null,
};
