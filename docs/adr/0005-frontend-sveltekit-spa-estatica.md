# ADR 0005: Frontend SvelteKit como SPA estática

- **Estado:** Aceptado
- **Fecha:** 2026-10-15

## Contexto

El panel de taut es pequeño: monitores, incidentes, canales de alerta y configuración del tenant,
siempre detrás de login. El [documento base](../taut.md) proponía SvelteKit + TypeScript + Tailwind
"compilado a estático para servir desde CDN" y dejaba Next.js como alternativa. La página de estado
pública la sirve el Status BFF (respuesta cacheada en Redis), no el panel.

## Decisión

- SvelteKit 3 (Svelte 5 con runes) + Tailwind 4 en `web/`, scaffoldeado con `sv create`.
- SPA estática: `@sveltejs/adapter-static` con `fallback: 'index.html'` y `ssr = false` en el
  layout raíz. El build (`web/build/`) se sirve desde un bucket o CDN; no hay servidor de
  SvelteKit en producción.
- La SPA habla con `taut-api` por HTTP con CORS; la URL se fija al construir con
  `PUBLIC_API_URL` (`web/.env.example`).
- Tooling según [ADR 0003](0003-tooling-just-lefthook-bun-oxc.md): bun, oxlint, oxfmt,
  svelte-check, vitest.

## Alternativas consideradas

- **Next.js:** más conocido, pero más pesado y orientado a SSR/React; el panel no necesita ni lo
  uno ni lo otro.
- **SvelteKit con adapter-node:** habilita SSR y endpoints de servidor, pero añade un servicio
  más que desplegar, escalar y observar, para un panel detrás de login donde el SEO no importa.
- **Vite + Svelte sin Kit:** más simple aún, pero se pierden las convenciones de rutas, layouts y
  carga de datos que harán falta cuando el panel crezca.

## Consecuencias

- El hosting debe devolver `index.html` para cualquier ruta desconocida (fallback de SPA); se
  configura en el bucket/CDN o en el Ingress cuando llegue el despliegue.
- Sin SSR no hay secretos en el frontend: todo lo que el build contiene es público, incluida la
  URL de la API. Los tokens de sesión (semana 13–14) viven en memoria o en cookies `HttpOnly`
  emitidas por el auth service, nunca en `localStorage`.
- La página raíz llama a `GET /health` y muestra el estado de la API: es la prueba de humo del
  circuito completo (CORS incluido) desde la semana 1.
- Si más adelante se necesita renderizado en servidor para alguna vista pública, se cambia el
  adapter en un ADR nuevo; el código de rutas no cambia.
