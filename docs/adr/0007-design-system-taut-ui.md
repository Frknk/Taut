# ADR 0007: Design system «Taut UI» basado en Kumo

- **Estado:** Aceptado; los componentes interactivos se apoyan en bits-ui desde la
  [ADR 0009](0009-bits-ui-componentes-accesibles.md)
- **Fecha:** 2026-10-16

## Contexto

El panel y la página de estado usaban colores primitivos de Tailwind (`neutral-*`, `red-50`,
`emerald-700`) repartidos por cada página, solo tenían tema claro y el logo era la palabra «Taut».
Cada pantalla nueva reinventaba botones, tarjetas y estados, y el ámbar de «Pausado» competía con
los colores que de verdad indican una falla.

## Decisión

- Un design system propio, «Taut UI», con referencia en
  [Kumo](https://kumo-ui.com/) (Cloudflare): tokens semánticos por rol (`canvas`, `base`,
  `elevated`, `text-subtle`…), todo el contenido a 14px, anillos en vez de bordes cuando hay
  sombra, radios concéntricos y hover sin transiciones. Se publica como artifact
  ([Taut UI](https://claude.ai/artifact/7E7UVV55bKyja5QwUtukoL)) con tokens, guías y previews.
- Marca monocroma: el color primario es la tinta del logo (`#09090B`, `#FAFAFA` en oscuro). El
  color queda para estados y enlaces. El logo aparece siempre sin fondo y sin texto; el fondo
  redondeado solo existe en el favicon.
- Geist y Geist Mono autoalojadas con `@fontsource-variable` (sin peticiones a Google desde el
  panel) e iconos de Phosphor (`phosphor-svelte`).
- Tokens en `web/src/routes/layout.css` con `light-dark()`: siguen al sistema operativo y
  `data-theme` en `<html>` fuerza uno. Se publican a Tailwind con los espacios
  `--background-color-*`, `--text-color-*` y `--color-*`, así que las clases son de rol:
  `bg-canvas`, `text-subtle`, `ring-line`. Los tokens `viz-*` y `status-*` validados no cambian
  en claro; su versión oscura sale de la misma paleta.
- Recetas en `web/src/lib/ui.ts` y componentes en `web/src/lib/components/` (Button, Badge,
  StatusBadge, Banner, LayerCard, Tabs, Select, Dialog, Empty, Logo).

### Reglas de diseño heredadas de Kumo

Las guías `kumo-design` de Cloudflare aplican a todo el frontend; el resumen operativo está en
`AGENTS.md`. Reglas: `content-text-size`, `heading-case`, `font-tracking`, `font-weight`,
`related-text-spacing`, `text-spacing`, `hover-color-transitions`, `shadow-borders`,
`concentric-border-radius`, `icon-alignment`, `inline-monospace-size`, `sticky-borders`,
`collapse-content-size`, `layer-card-nesting` y `dialog-rendering`. Excepciones documentadas: el
texto `text-xs` de badges, ayudas y botones pequeños, y el banner grande de la página pública.

### Movimiento

El movimiento sigue el criterio del curso *Animations on the Web* (skill `animate` del repo):
se anima solo lo que tiene un propósito (respuesta a una acción, un cambio de estado, evitar un
salto brusco) y según la frecuencia de uso.

- **Sin librerías.** Tokens y keyframes en `web/src/routes/layout.css` (`--ease-out`,
  `--ease-in-out`, `--ease-drawer` y utilidades `animate-*`), la utilidad `press` para botones y
  las transiciones de Svelte en `web/src/lib/motion.ts` (`step`, `pop`, `appear`, `leave`,
  `FLIP`).
- **Curvas y duraciones.** `ease-out` fuerte para lo que entra o sale, `ease-in-out` para lo que
  se mueve en pantalla, nunca `ease-in`. Menos de 300 ms (el drawer usa 300 ms con la curva de
  iOS); las salidas son más cortas que las entradas. Nada aparece desde `scale(0)`: se parte de
  0,96–0,97 con opacidad.
- **Overlays de bits-ui.** Se animan con `data-[state=open|closed]`: bits-ui espera a que la
  animación termine antes de desmontar, sin `forceMount`. Menús y selects crecen desde su
  disparador (`--bits-floating-transform-origin`); los diálogos desde el centro.
- **La landing** (`Landing.svelte`, la raíz para visitantes sin sesión) es la excepción
  documentada: se ve una vez, así que el titular entra palabra por palabra con `rise-up` (400 ms,
  12 px) y la tarjeta de ejemplo cuenta la historia de un monitor en tres segundos (la línea de
  latencia se dibuja con `draw`, el estado cambia, las barras entran escalonadas y aparece el
  aviso). Su párrafo de entrada va a 16px. Nada de lo que hay bajo el hero se mueve.
- **Qué no se anima.** Hovers de color (regla `hover-color-transitions`), la navegación del
  sidebar, el contenido de las tabs, los tooltips y el cursor de los gráficos (siguen al
  puntero), contadores y refrescos del polling. Entre páginas del panel solo hay un crossfade
  de 150 ms del contenido (`onNavigate` + View Transitions).
- **Movimiento reducido.** Con `prefers-reduced-motion` los mismos nombres de keyframes y las
  transiciones de `motion.ts` se vuelven fundidos de opacidad, y desaparecen los movimientos, la
  respiración del logo y las View Transitions.
- **Tests.** `expectAccessible()` espera a que terminen las animaciones antes de pasar axe.

## Alternativas consideradas

- **Usar Kumo directamente:** es una librería de React; el panel es Svelte.
- **shadcn-svelte o Skeleton:** traen su propio lenguaje visual y más componentes de los que el
  panel usa; adaptar sus tokens costaba más que escribir diez componentes pequeños.
- **Azul de Kumo como color de marca:** compite con los estados y con el anillo de foco; el logo
  ya es monocromo.

## Consecuencias

- Ningún componente usa colores primitivos ni la variante `dark:`; un cambio de paleta se hace en
  `layout.css` (y en el artifact) y llega a todo el panel.
- `light-dark()` exige navegadores de 2024 en adelante (Chrome 123, Safari 17.5, Firefox 120).
- Los borrados se confirman con `Dialog` en vez de `confirm()`.
- «Pausado» pasa a neutro con un glifo de pausa; cada estado tiene una forma distinta, así el
  color nunca es la única pista.
