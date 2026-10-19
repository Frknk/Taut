# ADR 0011: Pruebas de carga con k6 en modelo abierto, semilla SQL y Prometheus local

- **Estado:** Aceptado
- **Fecha:** 2026-10-19

## Contexto

El sílabo pide un reporte de k6 en la semana 6 (p95 y RPS de la api y de la página de estado),
repetirlo en la semana 13 con la caché de Redis y presentarlo en la compuerta de la semana 8.
`docs/taut.md` elegía k6 frente a Locust, pero sus tres escenarios (50 VUs, 500 VUs, 2 000
monitores) son anteriores al rate limit por tenant, al servicio auth y a RLS. Esos cambios
impiden lo obvio:

- Registrarse exige verificar el correo, y auth acepta 10 registros por IP y hora.
- Cada tenant tiene 600 peticiones/min (panel) y 10 monitores, y el único plan es `free`.
- Todo monitor creado se chequea de verdad.

Además, con la base vacía las páginas de estado y `/metrics` responden mucho más rápido que en
producción.

## Decisión

- **Modelo abierto.** Los escenarios usan `constant-arrival-rate` y `ramping-arrival-rate` (iteraciones
  por segundo), no un número de VUs. La carga no baja cuando Taut se pone lento: la lentitud se
  ve como latencia y como `dropped_iterations`, que también es un umbral. Los umbrales (p95 < 300 ms,
  el SLO de `docs/taut.md`) miran solo la fase `steady`.
- **Datos con una semilla SQL.** `just loadtest-seed` inserta con psql, como el rol dueño, usuarios
  ya verificados con una contraseña fija de desarrollo, tenants `lt-*`, monitores contra un
  servidor eco y la historia sintética de cada monitor: 89 días a 5 min para las páginas de estado
  y 30 días a 1 min para el panel (unos 16 M de filas). k6 solo hace login en `setup()`. La carga se
  reparte entre tenants para quedar por debajo del 80 % de su presupuesto.
- **Todo en compose.** k6 (`grafana/k6`, perfil `k6`) corre dentro de la red de compose contra las
  imágenes release, para no medir el reenvío de puertos del host. Prometheus y Grafana (perfil
  `obs`) reciben las métricas de k6 por remote write con histogramas nativos y scrapean las de
  los servicios. Los tableros viven en `infra/observability/`, y el de k6 (ID 18030) va
  vendorizado.
- **Reportes en el repo.** Va uno por corrida en `docs/loadtest/`, desde `TEMPLATE.md`, con la
  máquina, la configuración, la tabla por ruta, el lado servidor y los cuellos de botella.
- **Medir, no arreglar.** La línea base se toma con la configuración por defecto. Cada cuello de
  botella se reporta con su evidencia y se arregla en su propio PR, para que la comparación de la
  semana 13 solo cambie la caché.

## Alternativas consideradas

- **Registrarse por la api y leer el token en Mailpit:** ejercita el registro, pero obliga a activar
  `TRUST_PROXY` y falsear `X-Forwarded-For`, y es lento y frágil.
- **VUs fijos (modelo cerrado):** si el sistema se pone lento, los VUs esperan y la carga baja sola.
  Esconde justo lo que se quiere medir y hace injusta la comparación con y sin caché.
- **Solo el resumen de k6:** no muestra curvas en el tiempo ni deja cruzar la latencia del cliente con
  las conexiones de PostgreSQL o el lag del checker.
- **k6 nativo en macOS:** el tráfico pasa por el reenvío de puertos de la VM. Se usa solo contra
  staging.

## Consecuencias

- Las pruebas no dependen del PaaS ni de la nube, pero sus números son de una laptop en la que k6 y
  Taut comparten CPU. Por eso cada reporte anota la máquina y los recursos de la VM.
- La semilla tarda unos minutos y ocupa ~2,5 GB, y `just loadtest-clean` la borra.
- Prometheus y Grafana de compose son solo de desarrollo (sin login). En Kubernetes los
  reemplaza kube-prometheus-stack, que puede reusar los tableros.
- El job nightly contra staging de `docs/taut.md` queda pendiente hasta que exista staging.
