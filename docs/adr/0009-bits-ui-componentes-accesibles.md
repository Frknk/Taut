# ADR 0009: bits-ui como base de los componentes interactivos

- **Estado:** Aceptado
- **Fecha:** 2026-10-16

## Contexto

Los widgets del panel tenían el ARIA y el teclado escritos a mano. `Menu` buscaba sus ítems con
`querySelectorAll` y no tenía typeahead ni posicionamiento real. `Tabs` no tenía paneles. El drawer
móvil manejaba `inert`, el foco y Escape por su cuenta. La lista de incidentes anidaba un enlace
dentro de un botón. Las fechas completas vivían en `title=`, que no llega al teclado ni al táctil.
Cada componente nuevo repetía ese trabajo y nada comprobaba que el resultado fuera accesible.

## Decisión

- Los componentes interactivos de `web/src/lib/components/` se construyen sobre
  [bits-ui](https://bits-ui.com) 2.x: primitivas headless para Svelte 5, con el ARIA y el teclado
  de los patrones de WAI-ARIA y sin estilos propios. Taut UI ([ADR 0007](0007-design-system-taut-ui.md))
  sigue siendo el lenguaje visual: las recetas de `ui.ts` estilan los estados que bits-ui expone
  como atributos `data-*` (`data-highlighted`, `data-[state=open]`).
- El mapa es este:

  | Componente | Primitiva de bits-ui |
  | ---------- | -------------------- |
  | `Dialog` | `Dialog`, o `AlertDialog` con `alert` |
  | Drawer móvil | `Dialog` |
  | `Menu`, `MenuItem`, `MenuRadioItem` | `DropdownMenu` |
  | `Select` | `Select`, expuesto como `combobox` |
  | `Tabs`, `TabList`, `TabPanel` | `Tabs` |
  | `IncidentList` | `Accordion` |
  | Tipo y periodos de `MonitorForm` | `RadioGroup`, `Collapsible` |
  | `Tooltip` | `Tooltip` |
  | Vista de tabla de `LatencyChart` | `Toggle` |
- Donde bits-ui se queda corto, el wrapper lo completa y lo comenta en el código. Al contenido de
  `Tooltip` le agrega `role="tooltip"`. A la lista de `Select` le agrega `aria-labelledby` y
  `aria-controls`. A `Accordion` le agrega `role="region"` y lo enlaza con su disparador.
- Los componentes se prueban en Chromium con el modo navegador de Vitest
  (`@vitest/browser-playwright`), en archivos `*.svelte.test.ts`: teclado con `userEvent` y
  `expectAccessible()` (`web/src/lib/test/axe.ts`, axe-core con WCAG 2.2 A y AA).

## Alternativas consideradas

- **Seguir a mano:** cada widget nuevo repite foco, teclado y ARIA, y los errores solo aparecen al
  probar con un lector de pantalla.
- **melt-ui:** es la base de bits-ui, pero con builders en vez de componentes; bits-ui encaja mejor
  con wrappers que reciben snippets.
- **shadcn-svelte:** usa bits-ui por debajo, pero trae su propio estilo (descartado en la ADR 0007).
- **Mantener el `<select>` nativo:** es más accesible en móvil, pero no admite el mismo aspecto
  que los menús. El equipo eligió la consistencia visual.

## Consecuencias

- `Select` deja de ser nativo: en móvil ya no abre el picker del sistema, y la API pasa de
  `<option>` hijos a `items`.
- Los diálogos, menús y listas se montan en un portal en `<body>`. El tema llega igual, porque
  `data-theme` está en `<html>`.
- CI instala Chromium para los tests de componentes. En local hay que correr una vez
  `bunx playwright install chromium`.
- Los gráficos SVG conservan su patrón de slider propio: bits-ui no tiene una primitiva para
  ellos.
