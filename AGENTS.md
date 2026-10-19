# AGENTS.md: instrucciones para agentes de IA

Taut es un SaaS multi-tenant de monitoreo de disponibilidad (proyecto del curso IS04E104). Backend
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
| `crates/net`         | `taut-net`: `guarded_client()`, cliente reqwest que rechaza direcciones internas (checks y webhooks) |
| `crates/identity`    | `taut-identity`: `KeyPair` (firma Ed25519, JWKS), `Verifier`, `Claims`, tokens opacos, API keys y firma de webhooks (`webhook::sign`/`verify`); sin I/O |
| `services/api`       | `taut-api`: `lib.rs` construye el `Router` (`app()`), `main.rs` lo arranca; tests en `tests/` |
| `services/auth`      | `taut-auth`: registro, verificación, login, 2FA (`mfa.rs`: TOTP cifrado, códigos de respaldo), sesiones (Redis), reautenticación, recuperación y `/.well-known/jwks.json`; publica `users.*` |
| `services/scheduler` | `taut-scheduler`: líder por lock en Redis; publica `checks.requested` y evalúa heartbeats |
| `services/checker`   | `taut-checker`: consume `checks.requested`, un módulo por tipo en `src/checks/`, publica `checks.completed` |
| `services/incidents` | `taut-incidents`: guarda `checks.completed`, máquina de estados (`state.rs`), publica `incidents.*` |
| `services/notifier`  | `taut-notifier`: consume `incidents.*` y `notifications.test`; correo (lettre) y webhooks |
| `services/status-bff`| `taut-status-bff`: `GET /s/{slug}`, página de estado pública (sin destinos ni causas) |
| `Dockerfile`         | Imagen de cualquier servicio: `--build-arg SERVICE=<nombre>`                            |
| `web/`               | SvelteKit 3 + Tailwind 4, bits-ui, bun, oxlint, oxfmt, vitest (Node y Chromium); panel en `(app)/` con `Sidebar` (selector de organización), página pública en `status/[slug]`; recetas de estilo en `src/lib/ui.ts` |
| `migrations/`        | SQL de sqlx, una base de datos compartida en el MVP                                      |
| `compose.yaml`       | postgres (TimescaleDB), redis, nats, mailpit; los servicios bajo el perfil `app`         |
| `.github/workflows`  | `ci.yml` (PR y main) y `build.yml` (imagen a GHCR)                                       |

## Comandos

Usa siempre las recetas de `just` (`just --list`); no inventes comandos equivalentes.

- `just up` levanta la infraestructura local; los tests de integración la necesitan.
- `just check` es lo que corre CI: formato, clippy, oxlint, svelte-check, tests, cargo-deny,
  build del frontend. Tiene que estar en verde antes de dar algo por terminado.
- `just dev` y `just web` corren la api y el frontend; `just workers` corre scheduler, checker,
  incidents, notifier y status-bff juntos y `just run <servicio>` uno solo en su puerto;
  `just migrate-add <name>` crea migraciones; `just sqlx-prepare` regenera `.sqlx/` contra una base
  vacía (con la base de desarrollo la inferencia de nulabilidad cambia y CI falla).
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
- **Tenant y permisos**: los handlers de negocio reciben `CurrentTenant { tenant, actor }` y
  filtran toda query por `tenant_id`; nunca aceptan el tenant como parámetro del cuerpo o de la
  ruta (viaja en `X-Tenant-Id` y la api comprueba la membresía). Todo handler que escribe empieza
  con `actor.require(Permission::…)?` (`Monitors` para members, `Channels` y `Tenant` solo owners).
  Las acciones delicadas (canales, borrar el tenant, API keys) añaden `actor.require_recent_auth()?`
  (step-up: `403 reauth_required` si el último login o reautenticación tiene más de 5 min; el
  panel lo resuelve solo con `ReauthDialog`). Las rutas que no son de un tenant (`/v1/tenants`)
  reciben `CurrentUser`, que rechaza las API keys con `401`. `CurrentTenant` además cuenta la
  petición contra el presupuesto del tenant (`services/api/src/ratelimit.rs`, por plan: API keys
  y panel por separado) y responde `429` al agotarlo; no hace falta nada en el handler.
- **API keys**: `CurrentTenant` también acepta `Bearer taut_live_…` (`Actor::ApiKey`). Las claves
  solo llegan a las rutas de `API_KEY_ROUTES` en `services/api/src/tenant.rs` (monitores e
  incidentes); una ruta nueva queda cerrada a ellas hasta que se agregue ahí. Si el handler
  necesita al usuario, `actor.user_id()?` (`403` para una clave), nunca un valor por defecto.
- **Identidad** ([ADR 0008](docs/adr/0008-auth-service-eddsa-memberships.md)): contraseñas solo con
  `taut_auth::password` (Argon2id en `spawn_blocking`); secretos de un solo uso con
  `taut_identity::random` y guardados como SHA-256; nunca registrar tokens, cookies, códigos ni
  correos completos en los logs. Los correos de cuenta son eventos `AccountEmail` que envía el
  notifier. Segundo factor solo con `taut_auth::mfa`: el secreto TOTP se guarda cifrado
  (`TotpCipher`, `TAUT_AUTH__TOTP_KEY`), cada código se acepta una vez (`MfaStore::check_totp`) y
  en auth las rutas delicadas empiezan con `current.require_recent_auth()?`.
- **HTTP saliente**: toda petición hecha en nombre de un tenant (chequeos, webhooks) usa
  `taut_net::guarded_client`; nunca un `reqwest::Client` sin protección. Los errores de webhooks
  se registran con `without_url()`: la URL de Discord o Slack es un secreto. Los webhooks
  genéricos salen firmados con `taut_identity::webhook::sign` sobre los bytes exactos del cuerpo
  (`X-Taut-Signature`); el secreto (`signing_secret`) nunca se devuelve en lecturas ni va a los logs.
- **Configuración**: solo a través de `taut_config::Settings`. No leer `std::env::var` en los
  servicios; agregar el campo a `Settings`, al `Default` y a `.env.example`.
- **Logs**: `tracing` (`info!`, `warn!`, `error!` con campos estructurados). Nunca `println!`
  ni `dbg!`. El `trace_id` lo pone el middleware; no lo pases a mano.
- **Métricas**: con el crate `metrics` (`counter!`, `histogram!`); nombres `snake_case` con
  unidad al final (`_seconds`, `_total`).
- **Base de datos**: migraciones solo con `just migrate-add`, siempre compatibles hacia atrás.
  Toda tabla de negocio lleva `tenant_id`; las únicas excepciones son las de identidad (`users`,
  `user_tokens`), que pertenecen a una persona y no a un tenant. Tests de integración con
  `#[sqlx::test(migrator = "taut_db::MIGRATOR")]`; los de la api firman tokens con un `KeyPair`
  de prueba (`services/api/tests/common`), nunca hablan con auth.
- **Lints**: el workspace activa `clippy::pedantic` y `missing_docs`; documenta todo lo público y
  no silencies lints sin un comentario que diga por qué.
- **Estilos** en `web/` ([ADR 0007](docs/adr/0007-design-system-taut-ui.md)): solo clases de rol
  del design system [Taut UI](https://claude.ai/artifact/7E7UVV55bKyja5QwUtukoL) (`bg-canvas`,
  `bg-base`, `text-subtle`, `ring-line`, `bg-brand`…), declaradas en `src/routes/layout.css`.
  Nunca colores primitivos (`bg-neutral-50`, `text-red-700`) ni la variante `dark:`. Reusa los
  componentes de `src/lib/components/` (Button, Badge, Banner, LayerCard, Tabs/TabList/TabPanel,
  Select, Dialog, Menu/MenuItem/MenuRadioItem, Tooltip, Empty, Logo, TenantForm, Breadcrumb) y las recetas
  de `src/lib/ui.ts`; el logo va sin fondo ni texto.
  La navegación del panel vive en `Sidebar.svelte`: una sección nueva se agrega ahí, al grupo
  Monitoreo u Organización (lo personal va al menú de usuario del pie), y a `SECTIONS` de
  `src/lib/breadcrumb.ts` para la ruta de la barra superior (`Breadcrumb`). Las páginas anidadas
  no llevan enlace de volver: la ruta ya lo es, y nombran su recurso con `useBreadcrumbName`.
- **Reglas de Kumo** en `web/` (detalle en [ADR 0007](docs/adr/0007-design-system-taut-ui.md)):
  texto de contenido a 14px (16px o más solo en títulos); títulos en sentence case, sin
  `uppercase`; nunca `tracking-*` ni `font-bold` (`font-semibold` en títulos, `font-medium` en
  negrita en línea); texto relacionado más junto que el contenido que lo rodea; padding óptico
  (`px-5 py-4`, no `p-5`); hover sin `transition-colors`; sombra siempre con `ring ring-line`,
  nunca con `border`; radios concéntricos (exterior = interior + padding); iconos junto a texto
  multilínea en `h-lh flex items-center` dentro de `items-start`; monoespaciado en línea con
  `text-[0.9em]`; elementos `sticky` separados con `border`; contenido que se colapsa conserva su
  ancho; nunca un `LayerCard` dentro de otro; los `Dialog` se controlan con `open` y nunca se
  envuelven en `{#if}` (bits-ui ya monta el contenido solo mientras está abierto).
- **Accesibilidad** en `web/` ([ADR 0009](docs/adr/0009-bits-ui-componentes-accesibles.md)): todo
  widget interactivo (diálogo, menú, select, tabs, acordeón, tooltip, toggle) se construye sobre
  [bits-ui](https://bits-ui.com), nunca con ARIA o teclado a mano. Los estados se estilan con los
  `data-*` que expone (`data-highlighted`, `data-[state=open]`). Nada de `title=` para información:
  `Tooltip`. Cada componente tiene su `*.svelte.test.ts` (Vitest en Chromium) que prueba el
  teclado y termina con `expectAccessible()` de `src/lib/test/axe.ts`.
- **Gráficos** en `web/`: tokens de color en `src/routes/layout.css` (paleta validada), funciones
  puras en `src/lib/charts.ts`; todo gráfico lleva leyenda si tiene más de una serie, tooltip,
  navegación con teclado y una vista de tabla, y los colores de estado van con texto o icono.
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
