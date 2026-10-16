# ADR 0003: Tooling: just, lefthook, Conventional Commits, bun, oxlint y oxfmt

- **Estado:** Aceptado
- **Fecha:** 2026-10-15

## Contexto

El [documento base](../taut.md) hablaba de un Makefile (`make up`, `make test`, `make loadtest`),
de `npm test` en CI y de un `AGENTS.md` por repo para que los agentes de IA sigan las convenciones.
La máquina de desarrollo tiene `just`, `bun` y el Make 3.81 de macOS (2006); no tiene pnpm. Con
agentes de IA generando código, el feedback tiene que llegar antes del commit, no en el PR.

## Decisión

- **just** como único punto de entrada (`justfile`, `just --list`); carga `.env` automáticamente.
  No hay Makefile.
- **lefthook** gestiona los hooks de git: `pre-commit` corre `cargo fmt --check`, `oxlint` y
  `oxfmt --check` sobre archivos staged; `commit-msg` valida Conventional Commits con
  `scripts/check-commit-msg.sh`; `pre-push` corre `clippy -D warnings` y los tests unitarios.
- **Conventional Commits** con tipos `feat fix docs chore ci build refactor test perf revert` y
  scopes sugeridos `api web db http config telemetry infra ci docs`; mensajes en inglés.
- **bun** instala y lanza scripts en `web/`; Vite y SvelteKit corren sobre Node 24.
- **oxlint** y **oxfmt** en vez de ESLint y Prettier. Los `.svelte` se formatean con
  `prettier-plugin-svelte` a través de oxfmt (`"svelte": true` en `.oxfmtrc.json`).
- **cargo-nextest** para tests, **cargo-deny** para avisos, licencias y duplicados, **Dependabot**
  semanal agrupando minor y patch.
- Herramientas declaradas en `Brewfile`; `just setup` instala todo.

## Alternativas consideradas

- **Makefile:** funciona sin instalar nada, pero la sintaxis castiga, el Make de macOS es viejo y no
  hay `--list` con descripciones.
- **pre-commit (Python):** requiere Python y pip en cada máquina; lefthook es un binario.
- **husky + commitlint:** exige `node_modules` en la raíz de un repo que es mayormente Rust.
- **ESLint + Prettier:** más lentos y con más configuración; oxlint y oxfmt cubren lo que el
  proyecto necesita hoy.

## Consecuencias

- Quien clona el repo necesita Homebrew, rustup, bun y Docker/OrbStack; `just setup` hace el resto.
- oxlint solo lintea el bloque `<script>` de los `.svelte` (sin reglas de template); `svelte-check`
  cubre los tipos. Si hacen falta reglas de template se evalúa `eslint-plugin-svelte` u `oxvelte`.
- oxfmt está en beta (0.73): su salida puede diferir de Prettier en casos raros. Dependabot avisa
  de cada versión nueva; se revisa el diff de formato antes de aceptarla.
- Los hooks se pueden saltar con `LEFTHOOK=0`, solo en emergencias: CI vuelve a correr todo.
