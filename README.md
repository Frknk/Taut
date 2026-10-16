# taut

Servicio SaaS multi-tenant de monitoreo de disponibilidad: vigila sitios, APIs y servidores,
abre incidentes cuando algo cae, avisa por correo, Telegram o Discord y publica una página de
estado por tenant. Es el proyecto del curso IS04E104 (16 semanas) y cada unidad del sílabo se
materializa en una pieza del sistema: Docker y PaaS, Terraform, Kubernetes, multi-tenant, CQRS,
eventos, caché, JWT + 2FA, Prometheus y ELK.

Backend 100 % Rust (Axum, tokio, sqlx) en un Cargo workspace; frontend SvelteKit como SPA
estática; PostgreSQL 17 con TimescaleDB, Redis y NATS JetStream. El diseño completo está en
[docs/taut.md](docs/taut.md) y cada decisión importante en un [ADR](docs/adr/README.md).

**Estado:** prototipo 1 en construcción ([docs/prototipo-1.md](docs/prototipo-1.md)). Hechos los
hitos de la semana 3–4 (tenants, CRUD de monitores con pausa y cuotas, panel) y de la semana 5
(scheduler, checker e incidents sobre NATS: chequeos HTTP, palabra clave, TCP y heartbeat
guardados en TimescaleDB). Pendiente: incidentes, alertas, página de estado y el despliegue a un
PaaS.

## Requisitos

- macOS o Linux con [Homebrew](https://brew.sh) (para `just setup`).
- Rust vía [rustup](https://rustup.rs); `rust-toolchain.toml` fija la versión (1.98.1).
- [bun](https://bun.sh) y Node 24 para `web/`.
- Docker: [OrbStack](https://orbstack.dev) o Docker Desktop, con Compose v2.
- [just](https://github.com/casey/just) (lo instala `brew bundle` si falta).

## Arranque rápido

```bash
just setup   # brew bundle, sqlx-cli, cargo-watch, hooks de git, bun install, .env
just up      # postgres (TimescaleDB), redis y nats, con healthchecks
just dev     # taut-api en http://localhost:8080 (recompila al guardar)
just workers # scheduler, checker e incidents (en otra terminal)
just web     # panel en http://localhost:5173 (en otra terminal)
```

Cada servicio expone `/health`, `/ready` y `/metrics` en su puerto: api 8080, scheduler 8081,
checker 8082, incidents 8083 (los mismos que publica `just up-all`).

Comprobar:

```bash
curl -s localhost:8080/health   # {"status":"ok","service":"taut-api",...}
curl -s localhost:8080/ready    # {"status":"ready","checks":{"database":"ok"}}
curl -s localhost:8080/metrics  # formato Prometheus
```

## API

Las rutas de `/v1` que actúan sobre un tenant llevan la cabecera `X-Tenant-Id` con su UUID
(provisional hasta que exista autenticación, semana 13–14). Los errores son siempre
`{"error": {"code", "message", "trace_id"}}`.

| Método y ruta                         | Qué hace                                                    |
| ------------------------------------- | ----------------------------------------------------------- |
| `POST /v1/tenants`                    | Crea un tenant (`name`, `slug`)                             |
| `GET /v1/tenants` · `/v1/tenants/{id}` | Lista (provisional) o lee tenants                          |
| `GET /v1/monitors` · `POST`           | Lista o crea monitores del tenant                           |
| `GET/PUT/DELETE /v1/monitors/{id}`    | Lee, reemplaza la configuración o elimina un monitor        |
| `POST /v1/monitors/{id}/pause` · `/resume` | Pausa o reanuda                                        |
| `GET /v1/monitors/{id}/checks?limit=` | Últimos resultados (por defecto 50, máximo 500)            |
| `GET/POST /h/{token}` · `/h/{token}/fail` | Ping público de un heartbeat (sin cabecera de tenant) |

```bash
tenant=$(curl -s localhost:8080/v1/tenants -H 'content-type: application/json' \
  -d '{"name":"Acme","slug":"acme"}' | jq -r .id)
curl -s localhost:8080/v1/monitors -H "x-tenant-id: $tenant" -H 'content-type: application/json' \
  -d '{"name":"Sitio","type":"http","target":"https://example.com"}'
```

Antes de abrir un PR: `just check` (formato, clippy, oxlint, svelte-check, tests, cargo-deny,
build del frontend). Es exactamente lo que corre CI.

## Recetas de just

| Receta               | Qué hace                                                                    |
| -------------------- | --------------------------------------------------------------------------- |
| `just setup`         | Instala herramientas, hooks, dependencias de `web/` y crea `.env`           |
| `just up` / `up-all` | Levanta la infraestructura local; `up-all` construye y levanta también los servicios |
| `just down` / `nuke` | Apaga los contenedores; `nuke` borra también los volúmenes                  |
| `just logs [svc]`    | Sigue los logs de compose                                                   |
| `just dev`           | Corre `taut-api` con recarga al guardar                                     |
| `just workers`       | Corre scheduler, checker e incidents juntos                                 |
| `just run svc`       | Corre un servicio en su puerto local (`just run checker`)                   |
| `just web`           | Servidor de desarrollo del frontend                                         |
| `just fmt` / `fmt-check` | Formatea (o verifica) Rust y `web/`                                     |
| `just lint`          | `clippy -D warnings`, oxlint y svelte-check                                 |
| `just test`          | `cargo nextest` (necesita `just up`) y vitest                               |
| `just deny`          | cargo-deny: avisos de seguridad, licencias y duplicados                     |
| `just check` / `ci`  | Todo lo anterior más el build del frontend                                  |
| `just migrate`       | Aplica las migraciones pendientes                                           |
| `just migrate-add x` | Crea la migración `migrations/<timestamp>_x.sql`                            |
| `just db-reset`      | Recrea la base de datos y aplica las migraciones                            |
| `just sqlx-prepare`  | Regenera la caché offline `.sqlx/`                                          |
| `just image [svc]`   | Construye la imagen `taut-<svc>:dev` (por defecto `api`) con el commit actual |
| `just loadtest`      | Pruebas de carga con k6 (semana 6)                                          |

## Estructura

```
taut/
├── crates/
│   ├── config/      taut-config: Settings desde TAUT_* (figment) y .env
│   ├── telemetry/   taut-telemetry: logs JSON con trace_id, métricas Prometheus
│   ├── db/          taut-db: PgPool, migraciones embebidas, ping
│   ├── http/        taut-http: AppError, extractores, /health, /ready, /metrics, middleware, serve()
│   ├── model/       taut-model: tipos de dominio y eventos del bus compartidos
│   └── bus/         taut-bus: NATS JetStream (streams, publicación con trace_id, consumidores)
├── services/
│   ├── api/         taut-api: tenants, monitores (/v1) y pings de heartbeat (/h)
│   ├── scheduler/   taut-scheduler: planifica los chequeos (líder por lock en Redis)
│   ├── checker/     taut-checker: ejecuta HTTP, palabra clave y TCP
│   └── incidents/   taut-incidents: guarda los resultados en check_results
├── web/             SvelteKit 3 + Tailwind 4, SPA estática (bun, oxlint, oxfmt)
├── migrations/      migraciones sqlx compartidas (una base de datos en el MVP)
├── .sqlx/           caché offline de sqlx
├── infra/           terraform/ (sem. 5) y k8s/ (sem. 8)
├── loadtest/        escenarios k6 (sem. 6)
├── docs/            documento base, ADRs
├── .github/         ci.yml, build.yml, dependabot, plantilla de PR
├── Dockerfile       imagen de cualquier servicio (--build-arg SERVICE=...)
├── compose.yaml     postgres (TimescaleDB), redis, nats y los servicios (perfil app)
├── justfile         recetas de desarrollo
├── lefthook.yml     hooks de git
└── AGENTS.md        convenciones para agentes de IA (CLAUDE.md lo incluye)
```

## Configuración

Todo se configura por variables de entorno; `.env.example` documenta cada una y `just setup` lo
copia a `.env`. Las variables `TAUT_*` mapean a `taut_config::Settings` con `__` como separador
(`TAUT_HTTP__ADDR` → `http.addr`); `DATABASE_URL`, `REDIS_URL` y `NATS_URL` se leen también sin
prefijo porque sqlx-cli y las imágenes oficiales usan esos nombres.

## Documentación

- [docs/taut.md](docs/taut.md): documento base del proyecto (visión, arquitectura, roadmap).
- [docs/adr](docs/adr/README.md): decisiones de arquitectura.
- [CONTRIBUTING.md](CONTRIBUTING.md): ramas, commits, hooks y revisión.
- [AGENTS.md](AGENTS.md): instrucciones para agentes de IA que trabajan en el repo.

## Licencia

[MIT](LICENSE), © 2026 Frank Rivera y colaboradores de taut
([ADR 0006](docs/adr/0006-licencia-mit.md)). El repositorio es privado por ahora; la licencia ya
rige cuando se haga público.
