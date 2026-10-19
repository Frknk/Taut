# ADR 0010: Row-Level Security por tenant con un rol sin login

- **Estado:** Aceptado
- **Fecha:** 2026-10-18

## Contexto

El aislamiento entre tenants dependía solo de que cada query llevara `WHERE tenant_id = $1`. Una
query que lo olvide filtra datos de otros clientes sin que ningún test lo note. `docs/taut.md`
prometía Row-Level Security desde el diseño. Pero todos los servicios se conectaban como `taut`,
superusuario y dueño de las tablas, y ese rol se salta RLS aunque se use `FORCE`.

## Decisión

- La migración `row_level_security` crea `taut_app`: `NOLOGIN`, `NOSUPERUSER`, `NOBYPASSRLS`, con
  `GRANT` explícitos sobre las diez tablas de tenant y sobre `users (id, email, name)`. Cada tabla
  tiene RLS activo y la política `tenant_isolation`, que es
  `tenant_id = current_tenant_id()` (en `tenants`, `id`). La función lee `app.tenant_id`.
- `taut_db::begin_tenant(pool, tenant)` abre una transacción con `role = taut_app` y
  `app.tenant_id = tenant`, ambos `LOCAL`: se apagan con el commit.
- La api trabaja con `taut_db::restricted(&pool)`, que se conecta como `taut_app` desde el
  arranque (`-c role=taut_app`). Fuera de `state.tenant_tx(id)` no ve nada. `state.system_tx()`
  (`begin_system`) vuelve al dueño y solo se usa donde el tenant aún no se conoce:
  - la API key por su hash;
  - el heartbeat por su token;
  - la invitación por su enlace;
  - la lista de tenants de un usuario.
- incidents, notifier y status-bff abren `begin_tenant` con el tenant del evento o del slug. El
  scheduler y auth siguen como dueño, porque son transversales por naturaleza.
- `uptime_daily` es un continuous aggregate, y TimescaleDB no admite RLS en ellos. Además, leerlo
  exige `SELECT` del invocador para la marca de agua, lo que expondría a todos los tenants.
  `taut_app` no tiene permiso sobre él y lo lee con la función `SECURITY DEFINER`
  `uptime_daily_of_current_tenant(since)`.
- Las queries conservan su `WHERE tenant_id`: RLS es la segunda línea, no la única.

## Alternativas consideradas

- **Login separado para `taut_app`, con su propia `DATABASE_URL`:** es una barrera real incluso
  frente a una inyección SQL. Exige otra contraseña, un script de init en compose, CI y otra URL
  en cada servicio. Queda como trabajo futuro.
- **`SET LOCAL ROLE` solo dentro de las transacciones de tenant, sin pool restringido:** cambia
  menos código, pero olvidar la transacción vuelve al dueño y ve todo. El pool restringido falla
  cerrado: devuelve cero filas o un `42501`.
- **`FORCE ROW LEVEL SECURITY`:** no sirve con un superusuario y rompería al scheduler, a la
  retención y al refresco del aggregate.

## Consecuencias

- Protege de errores de código, no de una inyección. La sesión sigue siendo del dueño, así que
  `RESET ROLE` o `set_config` vuelven a abrirlo todo. Todas las queries son macros con parámetros.
- Cada transacción hace un viaje extra a la base; una petición del panel abre dos (membresía y
  handler). Con PgBouncer en modo transacción `begin_tenant` sigue valiendo (todo es `LOCAL`), pero
  el rol de arranque del pool (`-c role=taut_app`) no llega porque PgBouncer no reenvía `options`:
  antes de ponerlo delante, `taut_app` necesita un login propio.
- La migración necesita `CREATEROLE` o el rol creado de antemano, y tolera que ya exista (los roles
  son del clúster y las bases de test migran en paralelo).
- Una tabla de tenant nueva lleva `GRANT`, RLS y su política en la misma migración (lo exige
  `crates/db/tests/rls.rs`). TimescaleDB copia los permisos a los chunks, pero no las políticas:
  una query que nombre un chunk directamente se salta RLS (timescale/timescaledb#7830); el código
  nunca lo hace. Recrear un continuous aggregate sobre `check_results` exige desactivar RLS antes.
