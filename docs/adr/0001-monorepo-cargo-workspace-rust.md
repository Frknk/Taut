# ADR 0001: Monorepo con Cargo workspace y backend 100 % Rust

- **Estado:** Aceptado
- **Fecha:** 2026-10-15

## Contexto

El [documento base](../taut.md) proponía Rust (Axum + tokio + sqlx) como opción principal para los
ocho servicios del backend, dejaba TypeScript (Fastify + Prisma) como alternativa y preveía un
híbrido (Rust solo en el checker, TypeScript en la API) si el CRUD no avanzaba. El curso dura 16
semanas, el MVP debe estar en Kubernetes en la semana 8, y buena parte del esfuerzo va a
infraestructura (Terraform, K8s, observabilidad), no a código de aplicación. El checker es la pieza
con más carga: miles de chequeos HTTP concurrentes por réplica.

## Decisión

Backend 100 % Rust en un único Cargo workspace:

- `rustc` 1.98.1 fijado en `rust-toolchain.toml`, edición 2024, `resolver = "3"`.
- `crates/` para código compartido: `taut-config` (settings desde `TAUT_*`), `taut-telemetry`
  (logs JSON con `trace_id`, métricas Prometheus), `taut-db` (pool, migraciones embebidas, ping) y
  `taut-http` (`AppError`, `/health`, `/ready`, `/metrics`, middleware, `serve` con shutdown
  graceful).
- `services/<nombre>` con un binario por servicio; el primero es `taut-api`. Cada servicio nuevo
  copia el patrón de `services/api` (Dockerfile incluido) y reutiliza los crates.
- Dependencias y lints centralizados en el `Cargo.toml` raíz (`workspace.dependencies`,
  `workspace.lints` con `clippy::pedantic`, `unsafe_code = "forbid"`, `missing_docs = "warn"`).
- Migraciones de sqlx compartidas en `migrations/` (una base de datos en el MVP) y caché offline
  en `.sqlx/`.

## Alternativas consideradas

- **TypeScript 100 % (Fastify + Prisma/Drizzle):** CRUD más rápido de escribir, pero el checker
  dependería del event loop de Node y de más memoria por réplica; además rompe la premisa de "un
  solo lenguaje, un solo CI, un solo patrón de imagen" en menor medida que el híbrido pero pierde
  la ventaja técnica que motivó Rust.
- **Híbrido Rust (checker) + TypeScript (API):** dos toolchains en CI, dos patrones de Dockerfile
  y dos formas de leer configuración y escribir logs desde el día 1. Se mantiene como plan de
  contingencia, no como punto de partida.

## Consecuencias

- Un solo CI (`cargo fmt`, `clippy -D warnings`, `cargo deny`, `nextest`), un solo Dockerfile
  multi-stage con cargo-chef y una sola forma de configurar, loguear y servir HTTP.
- Tiempos de compilación más largos que en TypeScript; se mitigan con `Swatinem/rust-cache` en CI,
  cargo-chef en Docker y `opt-level = 1` para dependencias en el perfil dev.
- Riesgo asumido: el CRUD de monitores tarda más. **Fecha de revisión: semana 3.** Si el CRUD de
  monitores y tenants no está funcionando, se aplica la mitigación del documento base: la API pasa
  a TypeScript y el checker sigue en Rust (se registra en un ADR nuevo que reemplaza a este).
- Los crates compartidos son la frontera que todo servicio debe respetar: errores con `AppError`,
  configuración con `taut_config::Settings`, logs con `tracing`.
