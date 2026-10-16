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
| `crates/http`        | `taut-http`: `AppError`, extractores `Json`/`Path`/`Query`, routers de `/health`, `/ready`, `/metrics`, `layers::apply`, `serve` |
| `crates/model`       | `taut-model`: tipos de dominio compartidos (`MonitorKind`, `MonitorStatus`, eventos del bus, reglas de red) sin I/O |
| `crates/bus`         | `taut-bus`: NATS JetStream; `Bus::connect`, `publish` con `trace_id` en cabecera, `consume` para consumidores durables |
| `services/api`       | `taut-api`: `lib.rs` construye el `Router` (`app()`), `main.rs` lo arranca; tests en `tests/` |
| `services/scheduler` | `taut-scheduler`: líder por lock en Redis; publica `checks.requested` y evalúa heartbeats |
| `services/checker`   | `taut-checker`: consume `checks.requested`, un módulo por tipo en `src/checks/`, publica `checks.completed` |
| `services/incidents` | `taut-incidents`: consume `checks.completed` y lo guarda en `check_results` |
| `Dockerfile`         | Imagen de cualquier servicio: `--build-arg SERVICE=<nombre>`                            |
| `web/`               | SvelteKit 3 + Tailwind 4, bun, oxlint, oxfmt, vitest                                     |
| `migrations/`        | SQL de sqlx, una base de datos compartida en el MVP                                      |
| `compose.yaml`       | postgres (TimescaleDB), redis, nats; la api bajo el perfil `app`                         |
| `.github/workflows`  | `ci.yml` (PR y main) y `build.yml` (imagen a GHCR)                                       |

## Comandos

Usa siempre las recetas de `just` (`just --list`); no inventes comandos equivalentes.

- `just up` levanta la infraestructura local; los tests de integración la necesitan.
- `just check` es lo que corre CI: formato, clippy, oxlint, svelte-check, tests, cargo-deny,
  build del frontend. Tiene que estar en verde antes de dar algo por terminado.
- `just dev` y `just web` corren la api y el frontend; `just workers` corre scheduler, checker e
  incidents juntos y `just run <servicio>` uno solo en su puerto; `just migrate-add <name>` crea
  migraciones.
- Si el entorno no tiene Docker, di explícitamente qué tests no corriste.

## Convenciones obligatorias

- **Idioma** ([ADR 0004](docs/adr/0004-idioma-codigo-ingles-docs-espanol.md)): código,
  identificadores, logs, errores y commits en inglés; documentación, ADRs y textos de interfaz en
  español.
- **Commits** en formato Conventional Commits (`feat(api): ...`); el hook los valida. Tipos y
  scopes en [CONTRIBUTING.md](CONTRIBUTING.md).
- **Errores**: los handlers devuelven `Result<_, taut_http::AppError>`. Nunca `unwrap`/`expect`
  en código de producción fuera del arranque; nada de códigos de estado a mano. Usa los
  extractores `taut_http::{Json, Path, Query}` (no los de axum) para que la entrada inválida
  también responda con el JSON de error.
- **Tenant**: los handlers de negocio reciben `CurrentTenant` y filtran toda query por
  `tenant_id`; nunca aceptan el tenant como parámetro del cuerpo o de la ruta.
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
   `taut_db::connect` si usa la base de datos, `taut_bus::Bus::connect` si usa el bus →
   `Router` con `taut_http::health::router` (con un `Probe` por dependencia),
   `taut_http::metrics::router` y las rutas propias → `taut_http::layers::apply` →
   `taut_http::serve`, o `taut_http::serve_with_worker` si además consume del bus (ver
   `services/checker/src/main.rs`).
3. El `Dockerfile` de la raíz sirve para todos: agrega el servicio a `compose.yaml` (perfil
   `app`, con `SERVICE` en `build.args`), a la matriz de `.github/workflows/build.yml`, a la
   receta `port` del `justfile` y a `.claude/launch.json`.
4. Tests de integración en `services/<name>/tests/` usando `app()` con `tower::ServiceExt::oneshot`.
5. Actualiza el mapa de este archivo y el árbol del README.

## Cómo agregar un tipo de monitor

Es la tarea de la semana 12 (el tipo DNS, con un agente, vía PR). Pasos, en orden:

1. `crates/model/src/lib.rs`: la variante nueva en `MonitorKind` (también en `ALL` y
   `as_str`).
2. Migración con `just migrate-add <tipo>_monitors` que reemplaza el `CHECK` `monitors_kind_check`
   por uno que incluya el valor nuevo (los tipos son `text`, no `ENUM`), compatible hacia atrás.
3. API: la validación de `target` del tipo en `services/api/src/validate.rs`, con tests unitarios
   de casos válidos e inválidos, y los campos que no apliquen rechazados como en los demás tipos.
4. Checker: un módulo `services/checker/src/checks/<tipo>.rs` que implemente el trait `Check`
   (nunca falla: todo problema es un `Outcome::down` con causa legible), su brazo en
   `Checker::run` y tests en `services/checker/tests/` contra un servidor local, sin depender de
   internet. Respeta `allow_internal` como hacen `http.rs` y `tcp.rs`.
5. Web: la etiqueta en `KIND_LABELS` (`web/src/lib/format.ts`) y la validación en
   `web/src/lib/monitor-form.ts`, con su test.
6. Docs: README y `docs/prototipo-1.md`.

## Antes de abrir un PR

- `just check` en verde y, si tocaste queries de sqlx, `just sqlx-prepare`.
- README, ADR o este archivo actualizados si cambió el uso, una decisión o una convención.
- Descripción del PR según la plantilla: qué cambia, cómo probarlo, checklist.
