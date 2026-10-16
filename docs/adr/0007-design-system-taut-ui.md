# ADR 0007: Design system «taut UI» basado en Kumo

- **Estado:** Aceptado
- **Fecha:** 2026-10-16

## Contexto

El panel y la página de estado usaban colores primitivos de Tailwind (`neutral-*`, `red-50`,
`emerald-700`) repartidos por cada página, solo tenían tema claro y el logo era la palabra «taut».
Cada pantalla nueva reinventaba botones, tarjetas y estados, y el ámbar de «Pausado» competía con
los colores que de verdad indican una falla.

## Decisión

- Un design system propio, «taut UI», con referencia en
  [Kumo](https://kumo-ui.com/) (Cloudflare): tokens semánticos por rol (`canvas`, `base`,
  `elevated`, `text-subtle`…), todo el contenido a 14px, anillos en vez de bordes cuando hay
  sombra, radios concéntricos y hover sin transiciones. Se publica como artifact
  ([taut UI](https://claude.ai/artifact/7E7UVV55bKyja5QwUtukoL)) con tokens, guías y previews.
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
