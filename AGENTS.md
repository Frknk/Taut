# AGENTS.md: instrucciones para agentes de IA

taut es un SaaS multi-tenant de monitoreo de disponibilidad (proyecto del curso IS04E104). Backend
100 % Rust en un Cargo workspace, frontend SvelteKit como SPA estática, PostgreSQL 17 con
TimescaleDB, Redis y NATS JetStream. El diseño está en [docs/taut.md](docs/taut.md) y las decisiones
en [docs/adr](docs/adr/README.md); léelos antes de proponer cambios de arquitectura.

## Mapa del repositorio

| Ruta                 | Qué hay                                                                                  |
| -------------------- | ---------------------------------------------------------------------------------------- |
| `crates/config`      | `taut-config`: `Settings` desde variables `TAUT_*` (`__` anida) y `.env` fuera de prod  |
| `crates/telemetry`   | `taut-telemetry`: `init()` instala logs JSON con `trace_id` y el recorder de Prometheus |
| `crates/db`          | `taut-db`: `connect`, `migrate` (migraciones embebidas de `migrations/`), `ping`        |
| `crates/http`        | `taut-http`: `AppError`, routers de `/health`, `/ready`, `/metrics`, `layers::apply`, `serve` |
| `services/api`       | `taut-api`: `lib.rs` construye el `Router` (`app()`), `main.rs` lo arranca; tests en `tests/` |
| `web/`               | SvelteKit 3 + Tailwind 4, bun, oxlint, oxfmt, vitest                                     |
| `migrations/`        | SQL de sqlx, una base de datos compartida en el MVP                                      |
| `compose.yaml`       | postgres (TimescaleDB), redis, nats; la api bajo el perfil `app`                         |
| `.github/workflows`  | `ci.yml` (PR y main) y `build.yml` (imagen a GHCR)                                       |

## Comandos

Usa siempre las recetas de `just` (`just --list`); no inventes comandos equivalentes.

- `just up` levanta la infraestructura local; los tests de integración la necesitan.
- `just check` es lo que corre CI: formato, clippy, oxlint, svelte-check, tests, cargo-deny,
  build del frontend. Tiene que estar en verde antes de dar algo por terminado.
- `just dev` y `just web` corren la api y el frontend; `just migrate-add <name>` crea migraciones.
- Si el entorno no tiene Docker, di explícitamente qué tests no corriste.

## Convenciones obligatorias

- **Idioma** ([ADR 0004](docs/adr/0004-idioma-codigo-ingles-docs-espanol.md)): código,
  identificadores, logs, errores y commits en inglés; documentación, ADRs y textos de interfaz en
  español.
- **Commits** en formato Conventional Commits (`feat(api): ...`); el hook los valida. Tipos y
  scopes en [CONTRIBUTING.md](CONTRIBUTING.md).
- **Errores**: los handlers devuelven `Result<_, taut_http::AppError>`. Nunca `unwrap`/`expect`
  en código de producción fuera del arranque; nada de códigos de estado a mano.
- **Configuración**: solo a través de `taut_config::Settings`. No leer `std::env::var` en los
  servicios; agregar el campo a `Settings`, al `Default` y a `.env.example`.
- **Logs**: `tracing` (`info!`, `warn!`, `error!` con campos estructurados). Nunca `println!`
  ni `dbg!`. El `trace_id` lo pone el middleware; no lo pases a mano.
- **Métricas**: con el crate `metrics` (`counter!`, `histogram!`); nombres `snake_case` con
  unidad al final (`_seconds`, `_total`).
- **Base de datos**: migraciones solo con `just migrate-add`, siempre compatibles hacia atrás.
  Toda tabla de negocio lleva `tenant_id`. Tests de integración con
  `#[sqlx::test(migrator = "taut_db::MIGRATOR")]`.
- **Lints**: el workspace activa `clippy::pedantic` y `missing_docs`; documenta todo lo público y
  no silencies lints sin un comentario que diga por qué.
- **Secretos**: ninguno en el repositorio ni en imágenes; `.env` está ignorado y `.env.example`
  solo lleva valores de desarrollo.
- **Dependencias**: se declaran en `[workspace.dependencies]` del `Cargo.toml` raíz y los crates
  las usan con `workspace = true`. `cargo deny check` debe seguir en verde.

## Cómo agregar un servicio nuevo

1. `services/<name>/` con `Cargo.toml` (hereda `version`, `edition`, `lints` del workspace) y
   `src/main.rs` + `src/lib.rs` siguiendo `services/api`.
2. Arranque: `Settings::load()` → `taut_telemetry::init("taut-<name>", ...)` →
   `taut_db::connect` si usa la base de datos → `Router` con `taut_http::health::router`,
   `taut_http::metrics::router` y las rutas propias → `taut_http::layers::apply` →
   `taut_http::serve`.
3. Copia `services/api/Dockerfile` cambiando `taut-api` por el binario nuevo. Agrega el servicio a
   `compose.yaml` (perfil `app`) y, si publica imagen, un job en `.github/workflows/build.yml`.
4. Tests de integración en `services/<name>/tests/` usando `app()` con `tower::ServiceExt::oneshot`.
5. Actualiza el mapa de este archivo y el árbol del README.

## Cómo agregar un tipo de monitor

Pendiente hasta que exista el servicio `checker` (semana 7–8). Cuando exista, el lugar previsto
es un módulo por tipo en `services/checker/src/checks/` que implemente un trait común, más el
valor nuevo en el enum de tipos del `monitor service`, su validación de `target`, una migración si
cambia el esquema y tests que usen un servidor de eco local. Esta sección se completa en ese
momento; es la tarea de la semana 12 (agregar el tipo DNS con un agente, vía PR).

## Antes de abrir un PR

- `just check` en verde y, si tocaste queries de sqlx, `just sqlx-prepare`.
- README, ADR o este archivo actualizados si cambió el uso, una decisión o una convención.
- Descripción del PR según la plantilla: qué cambia, cómo probarlo, checklist.
