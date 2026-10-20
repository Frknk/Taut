# ADR 0012: Planes free, pro y business sin facturación

- **Estado:** Aceptado
- **Fecha:** 2026-10-19

## Contexto

Taut tenía un solo plan, `free`, con sus cuotas fijas en el código: 10 monitores, 10 heartbeats,
600 peticiones por minuto del panel, 60 con API key y 20 API keys activas. El modelo multi-tenant
del sílabo pide mostrar que cada organización tiene sus propios límites. Las pruebas de carga
dieron además una referencia de capacidad: el checker trabaja cómodo hasta unos 700 chequeos/s
([reporte](../loadtest/2026-10-19-checker-soak.md)). La facturación sigue fuera de alcance
(`docs/taut.md`).

## Decisión

Tres planes, con las cuotas en el enum `Plan` (`services/api/src/tenant.rs`):

| Límite                 | free | pro   | business |
| ---------------------- | ---: | ----: | -------: |
| Monitores              |   10 |    50 |      200 |
| Heartbeats             |   10 |    50 |      200 |
| API keys activas       |   20 |    20 |       50 |
| Peticiones del panel   |  600 | 1 200 |    3 000 |
| Peticiones con API key |   60 |   300 |      600 |

Las peticiones son por minuto y por organización.

- Free conserva exactamente las cuotas que tenía, así que ninguna organización existente cambia.
- Toda organización nueva empieza en `free`. Sin facturación, el plan lo cambia un operador con
  `just tenant-plan <slug> <plan>`; la api no ofrece ninguna ruta para cambiarlo.
- La api lee el plan en cada petición, así que el cambio vale desde la siguiente, sin reiniciar
  ni publicar eventos.
- Las respuestas de `GET /v1/tenants` y `GET /v1/tenants/{id}` traen `limits` con las cuotas
  del plan. El panel las muestra en lugar de tenerlas escritas: en Monitores y en Ajustes de la
  organización.
- La migración `tenant_plans` amplía el `CHECK` de `tenants.plan` a los tres valores. Es una
  expansión: toda fila existente sigue siendo válida.

## Alternativas consideradas

- **Selector de plan en el panel:** cualquier owner se pondría business gratis.
- **Ruta de administración en la api:** necesita un rol de plataforma que no existe todavía.
- **Cuotas en la configuración (`TAUT_*`):** las cuotas son parte del producto, no del
  despliegue; en el código quedan versionadas y con tests.

## Consecuencias

- Un business lleno (200 monitores a 30 s) genera 6,7 chequeos/s. Con el techo cómodo actual del
  checker caben unos 100 así; el arreglo de `taut_bus::consume` y el HPA de la semana 8 suben ese
  número.
- Cuando entre la facturación, el cambio de plan pasará de la receta a ese flujo. Las cuotas y
  `limits` no cambian.
- Los escenarios de carga siguen sembrando organizaciones `free`.
