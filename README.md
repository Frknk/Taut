# Taut

Servicio SaaS multi-tenant de monitoreo de disponibilidad: vigila sitios, APIs y servidores,
abre incidentes cuando algo cae, avisa por correo, Telegram o Discord y publica una página de
estado por tenant. Es el proyecto del curso IS04E104 (16 semanas) y cada unidad del sílabo se
materializa en una pieza del sistema: Docker y PaaS, Terraform, Kubernetes, multi-tenant, CQRS,
eventos, caché, JWT + 2FA, Prometheus y ELK.

Backend 100 % Rust (Axum, tokio, sqlx) en un Cargo workspace; frontend SvelteKit como SPA
estática; PostgreSQL 17 con TimescaleDB, Redis y NATS JetStream. El diseño completo está en
[docs/taut.md](docs/taut.md) y cada decisión importante en un [ADR](docs/adr/README.md).

**Estado:** prototipo 1 completo en código ([docs/prototipo-1.md](docs/prototipo-1.md)): las 17
features previstas para la compuerta de Medio Ciclo. Tenants y monitores HTTP, palabra clave, TCP y
heartbeat con pausa y cuotas; chequeos programados sobre NATS y guardados en TimescaleDB;
incidentes con periodos de confirmación y recuperación; alertas por correo, Discord, Slack y
webhook; página de estado pública con 90 días de historia; gráficos de latencia y uptime.
Semana 13–14 en curso: cuentas de usuario con verificación de correo, sesiones y organizaciones
con roles (este PR); después 2FA, invitaciones y API keys. Seguridad transversal: webhooks
firmados con HMAC-SHA256, presupuesto de peticiones por tenant en Redis y Row-Level Security en
PostgreSQL con un rol sin privilegios ([ADR 0010](docs/adr/0010-row-level-security-por-tenant.md)).
Pendiente: el despliegue (PaaS, Terraform y Kubernetes, semanas 5–8) y las pruebas de carga.

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
just workers # auth, scheduler, checker, incidents, notifier y status-bff (en otra terminal)
just web     # panel en http://localhost:5173 (en otra terminal)
```

Cada servicio expone `/health`, `/ready` y `/metrics` en su puerto: api 8080, scheduler 8081,
checker 8082, incidents 8083, notifier 8084, status-bff 8085, auth 8086 (los mismos que publica
`just up-all`). La página de estado pública de un tenant está en
http://localhost:5173/status/{slug}. Ningún correo sale de la máquina: Mailpit guarda las alertas
y los correos de cuenta (confirmación, recuperación) y los muestra en http://localhost:8025; ahí
está el enlace para confirmar el correo después de registrarse.

Si vienes de una versión anterior a la semana 13, los tenants existentes no tienen dueño:
`just db-reset` y vuelve a crearlos desde el panel.

Comprobar:

```bash
curl -s localhost:8080/health   # {"status":"ok","service":"taut-api",...}
curl -s localhost:8080/ready    # {"status":"ready","checks":{"database":"ok"}}
curl -s localhost:8080/metrics  # formato Prometheus
```

## API

Toda ruta de `/v1` lleva `Authorization: Bearer <access token>`; el token lo emite auth
(`POST /v1/auth/login`, 15 min) y se renueva con la cookie de sesión (`POST /v1/auth/refresh`).
Las rutas que actúan sobre un tenant llevan además la cabecera `X-Tenant-Id` con su UUID: la api
comprueba que el usuario es miembro y de ahí sale su rol (`owner` todo; `member` lee todo y
gestiona monitores). Las acciones delicadas (canales de alerta, borrar el tenant y crear API
keys) exigen además que el usuario haya confirmado su contraseña hace menos de 5 min:
si no, `403 reauth_required` y el panel pide la contraseña (`POST /v1/auth/reauth`) y reintenta.
Los errores son siempre `{"error": {"code", "message", "trace_id"}}`. Cada tenant tiene un
presupuesto por minuto (60 peticiones con API keys en el plan gratuito y 600 desde el panel);
pasado, la api responde `429 too_many_requests` con `Retry-After` en segundos.

Para scripts y CI hay API keys por tenant (`Authorization: Bearer taut_live_…`, sin
`X-Tenant-Id`: el tenant va en la clave). Las crea un owner en Configuración → API keys y se
muestran una sola vez. Una clave `read` solo hace `GET`; una `write` además gestiona monitores.
Solo llegan a monitores (con sus chequeos y métricas) e incidentes: miembros, canales, otras API
keys, `/v1/tenants` y el tenant les responden `401`/`403`. Revocada, responde `401` al instante.

| Método y ruta                         | Qué hace                                                    |
| ------------------------------------- | ----------------------------------------------------------- |
| `POST /v1/auth/register` (auth, :8086) | Crea la cuenta (`name`, `email`, `password`) y envía el correo de confirmación; siempre `202` |
| `POST /v1/auth/verify-email` · `/resend-verification` | Confirma el correo con el `token` del enlace, o pide otro enlace |
| `POST /v1/auth/login` · `/refresh` · `/logout` | Entra (`email`, `password`), renueva o cierra la sesión; el refresh viaja en la cookie `taut_refresh` |
| `GET /v1/auth/me` · `PUT /v1/auth/password` | Quién soy; cambia la contraseña (cierra las otras sesiones; pide reautenticación reciente) |
| `POST /v1/auth/forgot-password` · `/reset-password` | Recuperación por correo; siempre `202` |
| `POST /v1/auth/mfa/enroll` · `/confirm` · `/disable` · `/backup-codes` · `GET /v1/auth/mfa` | Verificación en dos pasos (TOTP): activar con el `code` de la app (devuelve 8 códigos de respaldo una sola vez), desactivar (`password` + `code`), regenerar códigos, estado |
| `POST /v1/auth/mfa`                   | Segundo paso del login: `mfa_token` (del `login` que respondió `mfa_required`) y `code` o `backup_code` |
| `POST /v1/auth/reauth`                | Reautenticación (`password` y `code` si hay 2FA): token con `auth_time` nuevo para las acciones delicadas |
| `GET /.well-known/jwks.json` (auth)   | Clave pública con la que la api verifica los tokens          |
| `POST /v1/tenants`                    | Crea un tenant (`name`, `slug`); quien lo crea es owner      |
| `GET /v1/tenants` · `/v1/tenants/{id}` | Mis tenants con mi rol, o uno de ellos                     |
| `GET /v1/members`                     | Miembros del tenant con su rol                              |
| `PUT/DELETE /v1/members/{user_id}`    | Cambia el rol (`role`) o quita a un miembro (owner); nunca al último owner (`409`) |
| `GET/POST /v1/invitations` · `DELETE /{id}` | Invitaciones pendientes; invitar (`email`, `role`; owner, máx. 20, enlace de 7 días por correo); revocar |
| `GET /v1/invitations/{token}` · `POST /{token}/accept` | Lo que muestra el enlace (sin sesión); aceptar con la cuenta del correo invitado |
| `GET/PUT/DELETE /v1/tenant` · `POST /v1/tenant/leave` | El tenant actual: ver, renombrar (`name`), borrar (owner + reautenticación) o salir |
| `GET/POST /v1/api-keys` · `DELETE /{id}` | API keys del tenant (owners): crear (`name`, `scope` = `read`/`write`; devuelve `key` una sola vez), listar con último uso, revocar |
| `GET /v1/monitors` · `POST`           | Lista o crea monitores del tenant                           |
| `GET/PUT/DELETE /v1/monitors/{id}`    | Lee, reemplaza la configuración o elimina un monitor        |
| `POST /v1/monitors/{id}/pause` · `/resume` | Pausa o reanuda                                        |
| `GET /v1/monitors/{id}/checks?limit=` | Últimos resultados (por defecto 50, máximo 500)            |
| `GET /v1/monitors/{id}/metrics?range=` | Uptime 24 h / 7 d / 30 d, p50 y p95, serie por intervalos (`24h`, `7d`, `30d`) |
| `GET /v1/incidents?status=&monitor_id=` | Incidentes (abiertos primero); `status` = `open`, `resolved` o `all` |
| `GET /v1/incidents/{id}`              | Un incidente con las alertas enviadas por canal            |
| `GET/POST /v1/alert-channels` · `GET/PUT/DELETE /{id}` | Canales: `email`, `discord`, `slack`, `webhook`; un `webhook` nuevo devuelve `signing_secret` una sola vez |
| `POST /v1/alert-channels/{id}/test`   | Envía un mensaje de prueba por el canal                    |
| `POST /v1/alert-channels/{id}/rotate-secret` | Nuevo secreto de firma de un `webhook` (owner + reautenticación); el anterior deja de valer |
| `GET/POST /h/{token}` · `/h/{token}/fail` | Ping público de un heartbeat (sin cabecera de tenant) |
| `GET /s/{slug}` (status-bff, :8085)   | JSON público de la página de estado; sin sesión            |

```bash
curl -s localhost:8086/v1/auth/register -H 'content-type: application/json' \
  -d '{"name":"Ana","email":"ana@taut.localhost","password":"correct horse battery"}'
# abre http://localhost:8025 y pulsa «Confirma tu correo» (o copia el token del enlace):
curl -s localhost:8086/v1/auth/verify-email -H 'content-type: application/json' -d '{"token":"…"}'
token=$(curl -s -c jar localhost:8086/v1/auth/login -H 'content-type: application/json' \
  -d '{"email":"ana@taut.localhost","password":"correct horse battery"}' | jq -r .access_token)
tenant=$(curl -s localhost:8080/v1/tenants -H "authorization: Bearer $token" \
  -H 'content-type: application/json' -d '{"name":"Acme","slug":"acme"}' | jq -r .id)
curl -s localhost:8080/v1/monitors -H "authorization: Bearer $token" -H "x-tenant-id: $tenant" \
  -H 'content-type: application/json' \
  -d '{"name":"Sitio","type":"http","target":"https://example.com"}'
curl -s -b jar -c jar -X POST localhost:8086/v1/auth/refresh   # nuevo access token y cookie rotada
```

Con una API key (créala en el panel o con `POST /v1/api-keys` justo después del login):

```bash
key=$(curl -s localhost:8080/v1/api-keys -H "authorization: Bearer $token" -H "x-tenant-id: $tenant" \
  -H 'content-type: application/json' -d '{"name":"CI","scope":"write"}' | jq -r .key)
curl -s localhost:8080/v1/monitors -H "Authorization: Bearer $key"   # taut_live_…
```

Los webhooks genéricos llegan firmados: `X-Taut-Signature: t=<unix>,v1=<hex>`, donde `v1` es el
HMAC-SHA256 de `t + "." + cuerpo` con el secreto `whsec_…` del canal. El receptor recalcula la
firma sobre los bytes exactos que recibió, rechaza `t` con más de 5 minutos de diferencia y
deduplica por `incident.id` + `event`, porque un reintento llega con otra firma:

```bash
# t y v1 salen de la cabecera; cuerpo.json son los bytes recibidos, sin reformatear
{ printf '%s.' "$t"; cat cuerpo.json; } | openssl dgst -sha256 -hmac "$secreto" -r   # = v1
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
| `just workers`       | Corre scheduler, checker, incidents, notifier y status-bff juntos           |
| `just run svc`       | Corre un servicio en su puerto local (`just run checker`)                   |
| `just web`           | Servidor de desarrollo del frontend                                         |
| `just fmt` / `fmt-check` | Formatea (o verifica) Rust y `web/`                                     |
| `just lint`          | `clippy -D warnings`, oxlint y svelte-check                                 |
| `just test`          | `cargo nextest` (necesita `just up`) y vitest (componentes en Chromium)     |
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
│   ├── bus/         taut-bus: NATS JetStream (streams, publicación con trace_id, consumidores)
│   ├── net/         taut-net: cliente HTTP saliente que rechaza direcciones internas
│   └── identity/    taut-identity: firma y verificación de tokens (Ed25519, JWKS), API keys
├── services/
│   ├── api/         taut-api: tenants, miembros, invitaciones, monitores (/v1) y pings (/h)
│   ├── auth/        taut-auth: registro, login (con 2FA TOTP), sesiones, reautenticación, JWKS
│   ├── scheduler/   taut-scheduler: planifica los chequeos (líder por lock en Redis)
│   ├── checker/     taut-checker: ejecuta HTTP, palabra clave y TCP
│   ├── incidents/   taut-incidents: guarda resultados, abre y cierra incidentes
│   ├── notifier/    taut-notifier: alertas por correo, Discord, Slack y webhook; correos de cuenta
│   └── status-bff/  taut-status-bff: JSON público de las páginas de estado
├── web/             SvelteKit 3 + Tailwind 4 + bits-ui, SPA estática (bun, oxlint, oxfmt)
├── migrations/      migraciones sqlx compartidas (una base de datos en el MVP)
├── .sqlx/           caché offline de sqlx
├── infra/           terraform/ (sem. 5) y k8s/ (sem. 8)
├── loadtest/        escenarios k6 (sem. 6)
├── docs/            documento base, ADRs
├── .github/         ci.yml, build.yml, dependabot, plantilla de PR
├── Dockerfile       imagen de cualquier servicio (--build-arg SERVICE=...)
├── compose.yaml     postgres (TimescaleDB), redis, nats, mailpit y los servicios (perfil app)
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

[MIT](LICENSE), © 2026 Frank Rivera y colaboradores de Taut
([ADR 0006](docs/adr/0006-licencia-mit.md)). El repositorio es privado por ahora; la licencia ya
rige cuando se haga público.
