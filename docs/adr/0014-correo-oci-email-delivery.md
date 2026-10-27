# ADR 0014: Correo de producción con OCI Email Delivery

- **Estado:** Aceptado
- **Fecha:** 2026-10-27

## Contexto

El notifier envía por SMTP las alertas y los correos de cuenta (verificación, recuperación,
invitaciones). En local los atrapa Mailpit, y en la VPS también: nadie los recibía, así que no se
podía registrar una cuenta real. `docs/prototipo-1.md` dejaba el proveedor por elegir. La VPS ya es
de Oracle Cloud (cuenta Free Tier, región de origen São Paulo) y el presupuesto es cero.

## Decisión

OCI Email Delivery por SMTP, sin cambios de código: el notifier arma el transporte de lettre desde
`TAUT_NOTIFIER__SMTP_URL`, que ya admite STARTTLS y credenciales.

- Endpoint `smtp.email.sa-saopaulo-1.oci.oraclecloud.com:587` con `?tls=required`. El usuario
  SMTP de OCI lleva `@`: en la URL va codificado (`%40`), igual que cualquier carácter especial de
  la contraseña.
- Un solo remitente aprobado, `Taut <no-reply@taut.lat>` (`TAUT_NOTIFIER__EMAIL_FROM`), dentro del
  dominio de correo `taut.lat` de OCI.
- DNS en Cloudflare: SPF `v=spf1 include:rp.oracleemaildelivery.com ~all`, DKIM con selector
  `taut-gru-20261027` (CNAME a OCI) y DMARC `v=DMARC1; p=none`.
- En la VPS las credenciales viven solo en `/opt/taut/.env` (`SMTP_URL`, `EMAIL_FROM`); Mailpit y
  su ruta `/mail` salen del despliegue. En local Mailpit sigue igual.

## Alternativas consideradas

- **Resend o Brevo:** planes gratis parecidos (100 y 300 correos al día), pero suman otra cuenta y
  otro proveedor a un despliegue que ya está entero en OCI.
- **Amazon SES:** más barato a escala, pero exige salir del sandbox con una solicitud y otra nube.
- **Un servidor SMTP propio en la VPS:** las IPs de nube suelen estar en listas negras, OCI
  bloquea el puerto 25 de salida y no hay memoria libre en 1 GB.

## Consecuencias

- La cuota gratis es de 100 correos al día (3 000 al mes). El notifier no limita su ritmo: manda
  hasta 32 a la vez y reintenta 5 veces cada 5 s, así que una ráfaga que choque con el límite por
  minuto de OCI podría perder correos. Si pasa, se agrega un límite de envío en el notifier.
- El captcha de [ADR 0013](0013-captcha-cloudflare-turnstile.md) protege la cuota en los
  formularios que envían correo.
- `taut.lat` no tiene buzón (sin MX): nadie lee las respuestas a `no-reply@`.
- Un dominio nuevo no tiene reputación: los primeros correos pueden caer en spam aun con SPF,
  DKIM y DMARC. Cuando haya historial, DMARC puede pasar a `p=quarantine`.
- Cambiar de región en OCI exige otro remitente aprobado y otra clave DKIM: ambos son por región.
