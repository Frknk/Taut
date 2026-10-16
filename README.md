# taut

Servicio SaaS multi-tenant de monitoreo de disponibilidad: vigila sitios, APIs y servidores,
abre incidentes cuando algo cae, avisa por correo, Telegram o Discord y publica una página de
estado por tenant. Es el proyecto del curso IS04E104 (16 semanas) y cada unidad del sílabo se
materializa en una pieza del sistema: Docker y PaaS, Terraform, Kubernetes, multi-tenant, CQRS,
eventos, caché, JWT + 2FA, Prometheus y ELK.

Backend 100 % Rust (Axum, tokio, sqlx) en un Cargo workspace; frontend SvelteKit como SPA
estática; PostgreSQL 17 con TimescaleDB, Redis y NATS JetStream. El diseño completo está en
[docs/taut.md](docs/taut.md) y cada decisión importante en un [ADR](docs/adr/README.md).

**Estado:** semana 1–2. Existe `taut-api` con `GET /health`, `GET /ready`, `GET /metrics` y
`GET /v1/ping`, la base de datos local con la extensión TimescaleDB, el frontend que muestra el
estado de la API, CI en GitHub Actions y la imagen Docker. Pendiente: el despliegue a un PaaS.

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
just web     # panel en http://localhost:5173 (en otra terminal)
```

Comprobar:

```bash
curl -s localhost:8080/health   # {"status":"ok","service":"taut-api",...}
curl -s localhost:8080/ready    # {"status":"ready","checks":{"database":"ok"}}
curl -s localhost:8080/metrics  # formato Prometheus
```

Antes de abrir un PR: `just check` (formato, clippy, oxlint, svelte-check, tests, cargo-deny,
build del frontend). Es exactamente lo que corre CI.

## Recetas de just

| Receta               | Qué hace                                                                    |
| -------------------- | --------------------------------------------------------------------------- |
| `just setup`         | Instala herramientas, hooks, dependencias de `web/` y crea `.env`           |
| `just up` / `up-all` | Levanta la infraestructura local; `up-all` construye y levanta también la api |
| `just down` / `nuke` | Apaga los contenedores; `nuke` borra también los volúmenes                  |
| `just logs [svc]`    | Sigue los logs de compose                                                   |
| `just dev`           | Corre `taut-api` con recarga al guardar                                     |
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
| `just image`         | Construye la imagen `taut-api:dev` con el commit actual                     |
| `just loadtest`      | Pruebas de carga con k6 (semana 6)                                          |

## Estructura

```
taut/
├── crates/
│   ├── config/      taut-config: Settings desde TAUT_* (figment) y .env
│   ├── telemetry/   taut-telemetry: logs JSON con trace_id, métricas Prometheus
│   ├── db/          taut-db: PgPool, migraciones embebidas, ping
│   └── http/        taut-http: AppError, /health, /ready, /metrics, middleware, serve()
├── services/
│   └── api/         taut-api: binario Axum, tests de integración, Dockerfile
├── web/             SvelteKit 3 + Tailwind 4, SPA estática (bun, oxlint, oxfmt)
├── migrations/      migraciones sqlx compartidas (una base de datos en el MVP)
├── .sqlx/           caché offline de sqlx
├── infra/           terraform/ (sem. 5) y k8s/ (sem. 8)
├── loadtest/        escenarios k6 (sem. 6)
├── docs/            documento base, ADRs
├── .github/         ci.yml, build.yml, dependabot, plantilla de PR
├── compose.yaml     postgres (TimescaleDB), redis, nats, api (perfil app)
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

Pendiente de decidir. El repositorio es privado por ahora; la licencia se elige antes de hacerlo
público.
