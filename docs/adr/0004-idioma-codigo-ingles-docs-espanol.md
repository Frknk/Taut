# ADR 0004: Idioma: código en inglés, documentación en español

- **Estado:** Aceptado
- **Fecha:** 2026-10-15

## Contexto

El [documento base](../taut.md) mezclaba idiomas: eventos con nombres en español
(`MonitorCreado`, `ResultadoChequeo`), tópicos del bus en inglés (`monitors.created`), tablas y
columnas en inglés (`alert_channels`, `tenant_id`). El curso se dicta en español; las librerías, los
mensajes de error de las herramientas y lo que producen los agentes de IA están en inglés.

## Decisión

- **En inglés:** identificadores, nombres de archivos de código, eventos y tópicos del bus,
  tablas y columnas, variables de entorno, mensajes de log y de error, comentarios dentro del
  código Rust y TypeScript, mensajes de commit y nombres de ramas.
- **En español:** README, ADRs, `CONTRIBUTING.md`, `AGENTS.md`, la documentación del curso en
  `docs/`, la plantilla de PR, y los comentarios de archivos de configuración orientados a personas
  (`justfile`, `compose.yaml`, workflows de CI, `Dockerfile`).
- **Interfaz de usuario en español:** los textos que ve el usuario del panel están en español
  porque el producto se presenta en el curso; si hace falta otro idioma se agrega i18n, no se
  traduce el código.
- Los nombres en español del documento base (`MonitorCreado`, `RegistrarResultado`) son
  descriptivos; en el código son `monitors.created`, `RegisterCheckResult`, etc.

## Alternativas consideradas

- **Todo en español:** choca con los nombres de las librerías (`Router`, `PgPool`), produce
  identificadores mixtos (`crearMonitorHandler`) y empeora lo que generan los agentes de IA, que
  están entrenados mayoritariamente en inglés.
- **Todo en inglés:** más portable si el repo se muestra fuera del curso, pero la documentación la
  leen el equipo y el docente en español, y traducirla no aporta.

## Consecuencias

- `AGENTS.md` fija la regla para los agentes; el hook de `commit-msg` valida el formato de los
  mensajes pero no el idioma: eso se revisa en el PR.
- Un lector del código no necesita español; un lector de la documentación no necesita inglés más
  allá de los identificadores.
- Si el repositorio se hace público para un público internacional, el README puede ganar una
  versión en inglés sin tocar el resto.
