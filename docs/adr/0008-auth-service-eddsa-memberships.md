# ADR 0008: Servicio auth separado, tokens EdDSA y membresías usuario–tenant

- **Estado:** Aceptado
- **Fecha:** 2026-10-16

## Contexto

Hasta la semana 12 no existían usuarios: la api resolvía el tenant con la cabecera `X-Tenant-Id`,
`GET /v1/tenants` listaba todos los tenants y cualquiera que conociera un UUID operaba sobre él.
Las semanas 13–14 del sílabo (JWT + 2FA) piden cerrar eso. El documento base
([docs/taut.md](../taut.md)) ya fijaba varias piezas: auth service propio, JWT de 15 min con
refresh opaco en Redis, Argon2id, TOTP, roles owner y member, API keys con hash en base de datos.
Dejaba abiertas, o fijaba de otra forma, cuatro cosas: el algoritmo de firma (decía RS256), la
relación usuario–tenant (decía `users.tenant_id`, un usuario en un solo tenant), dónde viaja el
tenant (decía dentro del JWT) y dónde vive el token en el navegador (el ADR 0005 prohíbe
`localStorage`).

## Decisión

Un servicio `services/auth` (`taut-auth`, puerto 8086) que comparte la base de datos y es dueño de
la identidad: registro, verificación de correo, login, sesiones, recuperación de contraseña, 2FA y
el JWKS. La api sigue siendo dueña de todo lo que pertenece a un tenant: tenants, miembros,
invitaciones y API keys.

- **Firma EdDSA (Ed25519)** en vez de RS256: claves de 32 bytes, sin padding ni parámetros que
  elegir mal, y el backend puro Rust de `jsonwebtoken` (RustCrypto) no arrastra aws-lc. auth
  publica la clave pública en `GET /.well-known/jwks.json`; la api la descarga al arrancar y la
  vuelve a pedir cuando un token nombra un `kid` desconocido, así una rotación no exige
  redesplegar. En desarrollo, sin clave configurada, auth genera una efímera: los tokens mueren
  con el proceso, las sesiones no (viven en Redis) y el panel se recupera con un refresh.
- **Membresías muchos a muchos**: `users` y `user_tokens` son tablas de identidad sin `tenant_id`
  (la única excepción a la regla de AGENTS.md) y `memberships(user_id, tenant_id, role)` une un
  usuario a cualquier número de tenants. Invitar a alguien que ya tiene cuenta no exige otra
  cuenta.
- **El tenant sigue en `X-Tenant-Id`**; el JWT solo dice quién es el usuario (`sub`, `sid`,
  `auth_time`, `mfa`, `ver`). `CurrentTenant` comprueba la membresía en cada petición y de ahí sale
  el rol: cambiar de organización no toca tokens y revocar un rol surte efecto al instante. Sin
  membresía la api responde `403 not_a_member`, exista o no el tenant.
- **Tokens en el navegador**: el access token vive solo en memoria y viaja como
  `Authorization: Bearer`; el refresh token es opaco, va en una cookie `HttpOnly; SameSite=Lax;
  Path=/v1/auth` (`Secure` fuera de dev) y en Redis queda su SHA-256. Se rota en cada uso y
  presentar uno ya rotado revoca la sesión entera. Cambiar o restablecer la contraseña sube
  `users.token_version` y mata todas las sesiones anteriores.
- **Correos por el bus**: auth y api publican eventos tipados (`users.*`, `tenants.*`, stream
  `ACCOUNTS`) y el notifier los redacta y envía; un solo sitio con SMTP y reintentos.
- Lo demás sigue el documento base: Argon2id, registro que no revela si un correo existe (siempre
  `202` y, si ya hay cuenta, un correo que lo dice), login bloqueado hasta verificar el correo,
  5 intentos fallidos por cuenta cada 15 min (20 por dirección IP: una oficina detrás de NAT no se
  bloquea por cinco errores ajenos), 2FA opcional con step-up (PR siguiente) y API keys
  `taut_live_…` por tenant (PR posterior; el `up_live_` del documento era un nombre heredado).

## Alternativas consideradas

- **Auth dentro de taut-api:** un despliegue menos y `CurrentTenant` ya vivía ahí, pero el
  documento base y el sílabo presentan auth como servicio; separarlo deja a la api sin
  contraseñas ni Redis de sesiones y permite verificar tokens en otros servicios con el JWKS.
- **RS256 + JWKS:** lo que decía el documento. Funciona igual, con claves de 2 KB y más formas de
  configurarlo mal; EdDSA es la recomendación actual para firmas nuevas.
- **tenant_id y role dentro del JWT:** ahorra una consulta indexada por petición, pero un rol
  revocado seguiría valiendo hasta 15 min y auth tendría que leer memberships.
- **Todo en cookies:** evita tocar tokens desde JavaScript, pero obliga a protección CSRF en toda
  ruta mutable y complica las API keys, que viajan como Bearer.
- **Sesiones en PostgreSQL:** más simple de inspeccionar; Redis ya estaba, el TTL es gratis y el
  documento lo asignaba ahí.

## Consecuencias

- Dos servicios con acceso de escritura a la misma base: auth escribe `users` y `user_tokens`, la
  api `tenants` y `memberships`. Las migraciones las aplica solo la api
  (`TAUT_DATABASE__MIGRATE_ON_START=false` en auth dentro de compose).
- `TAUT_HTTP__CORS_ORIGINS` tiene que ser una lista explícita: con `*` los navegadores no envían
  la cookie. Panel y auth deben compartir sitio (`localhost` con distintos puertos en desarrollo,
  un dominio registrable en producción) para que `SameSite=Lax` funcione.
- Detrás del ingress hace falta `TAUT_HTTP__TRUST_PROXY=true`, o todos los usuarios comparten el
  límite por IP.
- Los tenants creados antes de esta migración quedan sin owner: en desarrollo, `just db-reset`.
- Un `staging` configurado por error como `dev` enviaría cookies sin `Secure`; la variable
  `TAUT_APP__ENV` pasa a ser de seguridad, no solo de logs.
- Pendiente para los PRs siguientes: 2FA con TOTP y step-up por `auth_time`, invitaciones y
  ciclo de vida del tenant, API keys. (2FA y step-up llegaron en el PR 2; invitaciones, roles y
  borrado del tenant en el PR 3.)
