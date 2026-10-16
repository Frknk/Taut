# ADR 0002: TimescaleDB dentro de PostgreSQL para los resultados crudos

- **Estado:** Aceptado
- **Fecha:** 2026-10-15

## Contexto

Los resultados de chequeos son append-only y de alto volumen: a 1 000 monitores cada 60 s son
1,4 M filas por día (~250 MB/día) que deben retenerse 90 días. El [documento base](../taut.md)
proponía MongoDB 7 (colección con índice TTL) y dejaba TimescaleDB como "alternativa igual de
válida para el curso". La Unidad 1 del sílabo pide mostrar un caso SQL y uno NoSQL.

## Decisión

Guardar los resultados crudos en una hypertable de TimescaleDB dentro del mismo PostgreSQL 17 que
guarda la configuración:

- Imagen local `timescale/timescaledb:2.26.1-pg17` (no existe tag `latest` a propósito, para que
  un cambio de versión mayor de PostgreSQL nunca sea sorpresa).
- La migración `20261015000001_timescaledb.sql` crea la extensión; la hypertable `check_results`
  (particionada por `at`, con `tenant_id`) llega con el checker, junto con una política de
  retención de 90 días y agregados continuos para la proyección `uptime_daily`.
- En la nube, PostgreSQL gestionado con la extensión habilitada (o un StatefulSet con la misma
  imagen si el proveedor no la ofrece).

## Alternativas consideradas

- **MongoDB 7 autogestionado (réplica set):** segunda base de datos que operar, respaldar y
  describir en Terraform; segundo driver y segundo modelo de consultas en el código; para un
  equipo pequeño es el costo operativo más alto de todo el diseño.
- **MongoDB Atlas M0:** gratis y gestionado, pero servicio externo al clúster, con límites de
  almacenamiento (512 MB) que a 250 MB/día se agotan en dos días sin TTL agresivo.

## Consecuencias

- Una sola base de datos: un backup, un módulo de Terraform, un pool de conexiones, un lenguaje de
  consulta. sqlx cubre configuración y resultados con el mismo código.
- Row-Level Security se aplica también a los resultados, no solo a la configuración.
- Las proyecciones CQRS siguen siendo reconstruibles: borrar `uptime_daily` y recalcular desde la
  hypertable es la misma prueba que el documento describía con MongoDB.
- El caso NoSQL del sílabo se cubre con Redis (clave-valor) y NATS JetStream (streams
  persistentes), y la hypertable se presenta como almacenamiento orientado a series de tiempo
  (particionado por tiempo, compresión columnar, retención por política).
- A vigilar: tamaño del volumen local (`just nuke` lo limpia) y que la retención esté configurada
  antes del soak test de k6 (semana 6).
