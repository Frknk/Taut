# ADR 0013: Captcha con Cloudflare Turnstile en registro, login y recuperación

- **Estado:** Aceptado
- **Fecha:** 2026-10-27

## Contexto

Taut corre en una VPS pública (https://taut.lat) y tres formularios no piden sesión: registro,
login y "olvidé mi contraseña". Los límites de auth (5 contraseñas malas por cuenta, 20 por IP y
10 registros o recuperaciones por IP cada hora) frenan a un atacante con una sola dirección, no a
una red de bots. Además, registro y recuperación envían correo, y el plan gratis de OCI Email
Delivery permite 100 al día ([ADR 0014](0014-correo-oci-email-delivery.md)): un bot podría gastar
la cuota y dejar sin alertas a los usuarios. El DNS de `taut.lat` ya está en Cloudflare.

## Decisión

Cloudflare Turnstile en modo Managed en los tres formularios, verificado por taut-auth.

- El panel carga `https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit` una vez
  (`web/src/lib/turnstile.ts`) y cada formulario monta `Turnstile.svelte` con su `action`
  (`login`, `register`, `forgot_password`). El token viaja en el cuerpo como `captcha_token`.
- taut-auth lo verifica con `POST siteverify` (`services/auth/src/captcha.rs`) después de los
  límites baratos y antes de Argon2, la base de datos o el correo. Un token ausente o rechazado
  responde `403 captcha_failed`; si Cloudflare no contesta en 5 s, `503`.
- Un captcha fallido en el login cuenta contra la IP, no contra la cuenta: nadie bloquea una
  cuenta ajena sin resolverlo.
- Configuración: `TAUT_AUTH__TURNSTILE_SECRET` (secreto) y `PUBLIC_TURNSTILE_SITEKEY` (pública, al
  construir el panel). Sin secreto el captcha se apaga en desarrollo y tests; en staging y
  producción auth no arranca.
- Reenviar la verificación, el segundo factor y el reset con token no lo piden: ya los protege
  algo que un bot no tiene (una cuenta creada con captcha o un token del correo).

## Alternativas consideradas

- **reCAPTCHA de Google:** pide aceptar el seguimiento de Google y suele mostrar desafíos de
  imágenes; Turnstile casi nunca interrumpe a una persona y es gratis sin límite de verificaciones.
- **hCaptcha:** parecido en privacidad, pero su plan gratis muestra más desafíos y suma otro
  proveedor; Cloudflare ya administra el DNS.
- **Solo límites por IP:** no detienen una red de bots con muchas direcciones.

## Consecuencias

- En la VPS auth necesita salida a internet: está en las redes `backend` y `edge`.
- El widget es un iframe de Cloudflare, la única pieza interactiva del panel fuera de bits-ui
  ([ADR 0009](0009-bits-ui-componentes-accesibles.md)); su accesibilidad depende de Cloudflare.
- k6 no resuelve captchas: envía el token falso de Cloudflare, que auth ignora en local y acepta
  en la VPS solo con la clave de prueba (`loadtest/vps-demo.sh captcha prueba`). Esa clave llama
  a Cloudflare en cada login y suma su latencia a la medición.
- Si Cloudflare cae, nadie puede registrarse ni entrar (las sesiones abiertas siguen).
