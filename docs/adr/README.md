# Registros de decisiones de arquitectura (ADR)

Un ADR documenta una decisión que cuesta revertir: qué se decidió, en qué contexto, qué
alternativas había y qué consecuencias acepta el equipo. Se escribe cuando se toma la decisión, no
después, y no se edita para cambiarla: una decisión nueva reemplaza a la anterior con otro ADR.

## Convenciones

- Un archivo por decisión en esta carpeta: `NNNN-titulo-en-kebab-case.md`, con cuatro dígitos
  correlativos (`0001`, `0002`, ...).
- Estados: **Propuesto** (en discusión), **Aceptado** (vigente), **Reemplazado por ADR NNNN**.
- Plantilla: [0000-plantilla.md](0000-plantilla.md). Copiarla, numerar y completar las secciones;
  entre 25 y 60 líneas, sin relleno.
- En español, con identificadores, comandos y nombres de archivos en inglés tal como son
  ([ADR 0004](0004-idioma-codigo-ingles-docs-espanol.md)).
- Si un PR cambia una decisión registrada, incluye el ADR nuevo en el mismo PR y enlázalo desde
  el que reemplaza.

## Índice

| ADR  | Título                                                     | Estado   | Fecha      |
| ---- | ---------------------------------------------------------- | -------- | ---------- |
| 0001 | [Monorepo con Cargo workspace y backend 100 % Rust](0001-monorepo-cargo-workspace-rust.md) | Aceptado | 2026-10-15 |
| 0002 | [TimescaleDB dentro de PostgreSQL para los resultados](0002-timescaledb-en-postgresql.md)    | Aceptado | 2026-10-15 |
| 0003 | [Tooling: just, lefthook, Conventional Commits, bun, oxlint y oxfmt](0003-tooling-just-lefthook-bun-oxc.md) | Aceptado | 2026-10-15 |
| 0004 | [Idioma: código en inglés, documentación en español](0004-idioma-codigo-ingles-docs-espanol.md) | Aceptado | 2026-10-15 |
| 0005 | [Frontend SvelteKit como SPA estática](0005-frontend-sveltekit-spa-estatica.md)              | Aceptado | 2026-10-15 |
| 0006 | [Licencia MIT para el repositorio](0006-licencia-mit.md)                                     | Aceptado | 2026-10-15 |
