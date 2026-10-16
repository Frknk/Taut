# Prototipo 1: mapa de features

> Origen: la hoja de cálculo [uptime-features-betterstack](https://docs.google.com/spreadsheets/d/1loQVtI_vQw3cI_99krr74OrHG1fQ2LQDLlhWJdB02xg/edit)
> (pestañas Features, Resumen, Parámetros y Leyenda), un backlog de 56 features tomadas de Better
> Stack y clasificadas por tier (P0 = MVP, P1 = siguiente iteración, P2 = si sobra tiempo,
> ✗ = fuera de alcance) y esfuerzo (S = 1 pt, M = 3 pts, L = 8 pts). La pestaña Features está
> congelada en [backlog/features-2026-10-15.csv](backlog/features-2026-10-15.csv); la hoja sigue
> siendo el lugar donde se marca el avance (columna Estado).
>
> Alcance decidido el 15-10-2026: **las 12 features P0 más cinco P1 baratas** (keyword, puerto
> TCP, heartbeat, webhook genérico y sincronización incidente → status page). 17 features,
> 29 puntos. Ping, DNS y multi-región quedan para el prototipo 2.

## Qué es el prototipo 1

Un usuario crea monitores HTTP, keyword, TCP o heartbeat con nombre, frecuencia, timeout y
periodos de confirmación y recuperación; puede pausarlos. Workers los chequean en su intervalo y
guardan latencia, estado y mensaje de error. Cuando un monitor falla el tiempo configurado se abre
un incidente y se avisa por correo o webhook (Discord, Slack); cuando se recupera, se cierra y se
avisa de nuevo. Cada tenant tiene una página de estado pública con sus monitores, barras de 90 días
e incidentes abiertos, y un panel con gráficos de latencia y uptime por monitor.

Es lo que el documento base pide para la compuerta de Medio Ciclo (semana 8), con el alcance
afinado por la hoja. Las diferencias con el documento están al final.

## Parámetros del prototipo

Valores de la sección "B. Tu MVP" de la pestaña Parámetros; son los defaults del sistema y, donde
se indica, configurables por monitor.

| Parámetro                        | Valor                | Dónde vive                                              |
| -------------------------------- | -------------------- | ------------------------------------------------------- |
| Frecuencia por defecto (f)       | 60 s                 | `monitors.interval_s`, por monitor                       |
| Frecuencias permitidas           | 30 / 60 / 180 / 300 s | Validación en la API y selector en la UI               |
| Timeout                          | 10 s                 | `monitors.timeout_s`, por monitor; siempre < intervalo  |
| Periodo de confirmación (T_conf) | 60 s                 | `monitors.confirmation_period_s`, por monitor           |
| Periodo de recuperación (T_rec)  | 120 s                | `monitors.recovery_period_s`, por monitor               |
| Regiones desplegadas / quórum    | 3 / 2                | Prototipo 2 (F-18); el prototipo 1 corre en una región  |
| Cuota de monitores por tenant    | 10                   | Validación en `POST /v1/monitors` según `tenants.plan`  |
| Cuota de heartbeats por tenant   | 10                   | Idem, para `type = heartbeat`                           |
| Status pages por tenant          | 1                    | Una página por `tenants.slug`                           |
| Ventana de barras en status page | 90 días              | `uptime_daily` (agregado continuo de TimescaleDB)        |

Valores derivados (fórmulas propias a partir de los anteriores; la hoja los referencia como
"k checks" y "m checks"):

- **k = ⌈T_conf / f⌉ = 1** check de confirmación: el incidente se abre cuando, tras el primer
  fallo, el siguiente chequeo (60 s después) también falla. Es decir, **2 fallos consecutivos**.
- **m = ⌈T_rec / f⌉ = 2** checks de recuperación: el incidente se cierra tras 2 chequeos OK
  seguidos; un fallo intermedio reinicia la cuenta.
- Peor caso hasta abrir un incidente: f + timeout + T_conf + timeout ≈ **2 min 20 s**. Peor caso
  hasta cerrarlo: T_rec + f ≈ 3 min.
- Carga por tenant en el plan free: 10 monitores × 1 región × 1 440 chequeos/día =
  **14 400 filas/día** en `check_results`; con retención de 90 días, 1,3 M filas por tenant.

## Mapa feature → taut

Columnas: dónde vive (servicio o crate), qué datos toca, qué API o pantalla expone, en qué semana
del roadmap entra y cómo se da por hecha. Los servicios son los del [documento base](taut.md):
`api` existe; `scheduler`, `checker`, `incidents`, `notifier` y `status-bff` nacen en estas semanas
siguiendo el patrón de `services/api`.

### Tipos de monitor

| ID   | Feature         | Tier / pts | Dónde vive                              | Datos                                                                 | API / UI                                                      | Sem. | Criterio de aceptación                                                                 |
| ---- | --------------- | ---------- | --------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------- | ---- | -------------------------------------------------------------------------------------- |
| F-01 | HTTP status     | P0 / 1     | `checker` (tokio + reqwest)             | `monitors.type = http`, `target`, `expected_status` (default 2xx); `check_results` | Tipo en el formulario de monitor                      | 5    | Un monitor a `httpbin.org/status/500` abre incidente; a `/status/200` queda UP         |
| F-02 | Keyword         | P1 / 1     | `checker`                               | `monitors.type = keyword`, `keyword`                                  | Campo "texto esperado" cuando el tipo es keyword              | 5    | Falla si el body no contiene el texto aunque el status sea 200                         |
| F-03 | Heartbeat / cron | P1 / 3    | `api` (endpoint público de ingesta) + `scheduler` (barrido de vencidos) | `monitors.type = heartbeat`, `heartbeat_token`, `grace_period_s`, `last_ping_at`; el barrido escribe `check_results` | `GET/POST /h/{token}` y `/h/{token}/fail`; estado "Pendiente" hasta el primer ping | 5 | Sin ping en `interval_s + grace_period_s` abre incidente; `/fail` lo abre al instante |
| F-04 | Puerto TCP      | P1 / 1     | `checker` (`TcpStream::connect` con timeout) | `monitors.type = tcp`, `target = host:puerto`                    | Tipo en el formulario                                         | 5    | Puerto cerrado o host inalcanzable → fallo con causa "connection refused" o "timeout"  |

### Configuración del monitor

| ID   | Feature                   | Tier / pts | Dónde vive                          | Datos                                                                       | API / UI                                                     | Sem. | Criterio de aceptación                                                                 |
| ---- | ------------------------- | ---------- | ----------------------------------- | --------------------------------------------------------------------------- | ------------------------------------------------------------ | ---- | -------------------------------------------------------------------------------------- |
| F-22 | Nombre del monitor        | P0 / 1     | `api`                               | `monitors.name` (nuevo respecto al documento base)                          | Campo obligatorio; se muestra en lista, alertas y status page | 3–4  | No se puede crear un monitor sin nombre; el nombre sale en el correo de alerta         |
| F-14 | Frecuencia de chequeo     | P0 / 1     | `api` (validación) + `scheduler`    | `monitors.interval_s ∈ {30, 60, 180, 300}`, default 60                     | Selector con cuatro valores                                  | 3–4  | La API rechaza 45 s; el scheduler respeta el intervalo con lag < 10 s                  |
| F-15 | Timeout                   | P0 / 1     | `api` (validación) + `checker`      | `monitors.timeout_s`, default 10                                            | Campo numérico                                               | 3–4  | La API rechaza `timeout_s >= interval_s`; un target que tarda 15 s falla por timeout   |
| F-16 | Periodo de confirmación   | P0 / 3     | `incidents` (máquina de estados)    | `monitors.confirmation_period_s`, default 60 → k checks                     | Campo avanzado con el default visible                         | 6    | Con f = 60 y T_conf = 60, el incidente se abre al segundo fallo consecutivo, no al primero |
| F-17 | Periodo de recuperación   | P0 / 3     | `incidents`                         | `monitors.recovery_period_s`, default 120 → m checks                        | Campo avanzado                                               | 6    | Se cierra tras m OK seguidos; un fallo intermedio reinicia la cuenta                   |
| F-25 | Pausar / reanudar         | P0 / 1     | `api` + `scheduler` + `incidents`   | `monitors.active` (ya previsto)                                             | `POST /v1/monitors/{id}/pause` y `/resume`; botón en la lista | 3–4  | Pausado: no se chequea, no abre incidentes, la status page lo muestra como "pausado"   |

### Evidencia y métricas

| ID   | Feature                        | Tier / pts | Dónde vive                                   | Datos                                                                          | API / UI                                                               | Sem. | Criterio de aceptación                                                                |
| ---- | ------------------------------ | ---------- | -------------------------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------- | ---- | ------------------------------------------------------------------------------------- |
| F-27 | Mensaje de error               | P0 / 1     | `checker`                                    | `check_results.status` (código HTTP) y `check_results.error` (texto: timeout, dns, connection refused, status 500, keyword missing) | Visible en el detalle del monitor y en el incidente | 5 | Cada fallo guarda causa legible; el incidente muestra la del primer fallo              |
| F-26 | Gráficos de latencia y uptime  | P0 / 3     | Lecturas dentro de `api` (la separación CQRS llega en la sem. 10) | `check_results` con `time_bucket`; `uptime_daily` (agregado continuo)  | `GET /v1/monitors/{id}/metrics?range=24h\|7d\|30d`; gráfico de latencia y % de uptime en el detalle | 7 | Un monitor con 24 h de datos muestra p50/p95 de latencia y uptime del día, semana y mes |

### Incidentes

| ID   | Feature                   | Tier / pts | Dónde vive   | Datos                                                                       | API / UI                                             | Sem. | Criterio de aceptación                                                                      |
| ---- | ------------------------- | ---------- | ------------ | --------------------------------------------------------------------------- | ---------------------------------------------------- | ---- | ------------------------------------------------------------------------------------------- |
| F-32 | Abrir/cerrar automático   | P0 / 3     | `incidents` (consume `checks.completed`, emite `incidents.opened` / `incidents.resolved`) | `incidents(id, tenant_id, monitor_id, started_at, resolved_at, cause)`; estado del monitor en Redis con CAS para no duplicar | `GET /v1/incidents`; lista de incidentes abiertos en el panel | 6 | Apagar el target abre un incidente una sola vez aunque haya dos réplicas; encenderlo lo cierra |

### Alertas

| ID   | Feature                                  | Tier / pts | Dónde vive  | Datos                                                                                  | API / UI                                                   | Sem. | Criterio de aceptación                                                                 |
| ---- | ---------------------------------------- | ---------- | ----------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------- | ---- | -------------------------------------------------------------------------------------- |
| F-40 | Email                                    | P0 / 1     | `notifier` (consume `incidents.*`) | `alert_channels.kind = email`, `config_json.to`; en local, Mailpit en compose; en la nube, un proveedor SMTP/API por decidir | CRUD de canales en `/v1/alert-channels`; pantalla de canales | 6 | Abrir y cerrar un incidente envía dos correos con nombre del monitor, causa y enlace |
| F-41 | Webhook genérico (Slack / Teams / Discord) | P1 / 1   | `notifier`  | `alert_channels.kind ∈ {webhook, discord, slack}`, `config_json.url`; plantilla de payload por kind (`{"content"}` Discord, `{"text"}` Slack, JSON completo genérico) | Igual que email | 7 | Un webhook de Discord recibe el mensaje al abrir y al cerrar; la firma HMAC llega en la sem. 13–14 |

### Status page

| ID   | Feature                   | Tier / pts | Dónde vive                                              | Datos                                                                     | API / UI                                                                | Sem. | Criterio de aceptación                                                            |
| ---- | ------------------------- | ---------- | ------------------------------------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------------------- | ---- | --------------------------------------------------------------------------------- |
| F-46 | Página pública            | P0 / 3     | `status-bff` (JSON público) + ruta pública de la SPA    | `tenants.slug`, monitores activos, `uptime_daily` de 90 días              | `GET /s/{slug}` en el BFF; `/status/{slug}` en `web/` con lista y barras de 90 días | 7 | Sin sesión se ve la página del tenant; nunca expone targets ni IPs; dos tenants no se mezclan |
| F-47 | Sync incidente → página   | P1 / 1     | `status-bff`                                            | Estado por monitor derivado de `incidents.resolved_at IS NULL`           | Bloque "incidentes activos" y color del monitor                         | 7    | Abrir un incidente cambia la página en el siguiente refresco (caché Redis de 30 s en la sem. 13) |

### Lo que el prototipo 1 ya tiene o recibe gratis

- **F-52 API REST (P1):** la hoja la anota como "casi gratis si el frontend consume el backend".
  Es el caso: todo lo anterior son endpoints de `/v1`, la SPA no tiene otro camino.
- **Tenants:** no es una feature de la hoja pero sí del documento base; el CRUD de tenants y la
  cabecera de tenant provisional entran en la semana 3–4 y son prerequisito de todo lo demás.

## Plan por semanas

Puntos según la hoja (S = 1, M = 3). La semana 6 es la crítica.

| Sem. | Compuerta        | Features                                                     | Pts | Infra en paralelo (roadmap)          |
| ---- | ---------------- | ------------------------------------------------------------ | --- | ------------------------------------ |
| 3–4  | Parcial 1        | F-22, F-14, F-15, F-25 y los campos de F-16/F-17 (CRUD de tenants y monitores, panel) | 4 | PaaS con `/health` (pendiente de la sem. 1–2) |
| 5    |                  | F-01, F-02, F-04, F-27 (scheduler + checker + `check_results`), F-03 (heartbeat) | 7 | Terraform                             |
| 6    |                  | F-32, F-16, F-17 (incidents), F-40 (notifier con email)      | 10  | k6 base                              |
| 7    |                  | F-46, F-47 (status-bff + ruta pública), F-26 (gráficos), F-41 (webhooks) | 8 | API Gateway + BFF balanceado      |
| 8    | Medio Ciclo      | Integración en Kubernetes, demo, buffer                      | 0   | Manifiestos K8s                      |
|      | **Total**        | **17 features**                                              | **29** |                                   |

Mínimo viable si la semana 6 o 7 aprieta: `scheduler` y `checker` pueden salir como un solo
binario, e `incidents` y `notifier` como otro; la separación completa en seis servicios puede
esperar a la semana 11 (microservicios por eventos) sin cambiar los tópicos del bus.

## Cambios al modelo de datos respecto al documento base

| Tabla           | Cambio                                                                                                   | Motivo                          |
| --------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `monitors`      | Nuevos: `name`, `confirmation_period_s`, `recovery_period_s`, `heartbeat_token`, `grace_period_s`, `last_ping_at`; `type` gana `heartbeat`; `interval_s` restringido a {30, 60, 180, 300} | F-22, F-16, F-17, F-03, F-14 |
| `check_results` | Hypertable en TimescaleDB (sustituye al documento de MongoDB): `monitor_id, tenant_id, at, region, ok, status, latency_ms, error, trace_id`; retención 90 días | [ADR 0002](adr/0002-timescaledb-en-postgresql.md), F-27 |
| `uptime_daily`  | Agregado continuo sobre `check_results` en vez de tabla mantenida a mano                                 | F-26, F-46                      |
| `alert_channels`| `kind` admite `email`, `webhook`, `discord`, `slack`; `telegram` queda para el prototipo 2               | F-40, F-41                      |
| `incidents`     | Sin cambios; `acknowledged_at/by` y `resolved_by` (F-33) llegan en el prototipo 2                        | F-32                            |

## Diferencias con el documento base y cómo se resuelven

| Tema                      | Documento base (14-10)                              | Hoja de features                                   | Prototipo 1                                                                 |
| ------------------------- | --------------------------------------------------- | -------------------------------------------------- | --------------------------------------------------------------------------- |
| Tipos de monitor del MVP  | HTTP, TCP, ping, DNS, keyword                       | Solo HTTP en P0; TCP y keyword P1; ping P1 (M); DNS P2 | HTTP, keyword, TCP, heartbeat. Ping va al prototipo 2 (riesgo ICMP/NET_RAW). DNS se mantiene como la tarea con agente de IA de la **semana 12** |
| Regla para abrir incidente | "3 fallos seguidos = DOWN"                         | Periodo de confirmación configurable (T_conf)      | Configurable por monitor; con los defaults de la hoja son 2 fallos seguidos. Quien quiera 3 pone T_conf = 120 s |
| Intervalos                | 20 s a 5 min                                        | Valores fijos 30 / 60 / 180 / 300 s                | Valores fijos de la hoja                                                    |
| Canales de alerta         | Correo, Telegram, Discord                           | Email P0, webhook genérico P1, push/SMS ✗          | Email y webhook (Discord y Slack por incoming webhook). Telegram (bot API) al prototipo 2 |
| Multi-región              | "Uno o más puntos de prueba"                        | P1, esfuerzo L, "valor curso nube: sí, primero"    | Prototipo 2: necesita el clúster y es el mejor escaparate de Kubernetes multi-región (sem. 8–9) |
| Página de estado          | Servida por el BFF, cacheada en Redis               | P0 mínima: lista + barras de 90 días               | BFF entrega JSON y la SPA lo pinta; la caché Redis llega en la sem. 13      |

## Candidatas para el prototipo 2

Ordenadas por la columna "Orden tier" y puntos de la hoja, con la semana del roadmap donde encajan:

| ID   | Feature                          | Tier / pts | Encaja en                                                        |
| ---- | -------------------------------- | ---------- | ---------------------------------------------------------------- |
| F-18 | Multi-región + quórum (3 regiones, caído si fallan 2) | P1 / 8 | Sem. 8–9, despliegue multi-región de `checker`; "sí, primero" para el curso |
| F-07 | Certificado SSL                  | P1 / 1     | Primero en entrar si sobra tiempo según el documento base         |
| F-06 | Ping (ICMP)                      | P1 / 3     | Cuando se decida NET_RAW o TCP connect como sustituto             |
| F-19 | Método, params, headers, HTTP auth | P1 / 1   | Con el CRUD maduro                                               |
| F-20 | Verificar SSL                    | P1 / 1     | Junto a F-19                                                      |
| F-23 | Badge de GitHub                  | P1 / 1     | SVG dinámico desde `status-bff`                                   |
| F-28 | Timeline del incidente           | P1 / 3     | Sale de `check_results`; con F-33                                 |
| F-33 | Acknowledge / resolve con autor  | P1 / 1     | Requiere usuarios autenticados (sem. 13–14)                       |
| F-48 | Updates, mantenimiento, degradación | P1 / 3  | Status page v2                                                    |
| F-09 | DNS                              | P2 / 3     | **Sem. 12**: tipo de monitor agregado con agente de IA vía PR     |
| F-53 | Equipos / roles                  | P2 / 3     | Sem. 9 (multi-tenant) y 13–14 (auth)                              |
| F-44 | Escalamiento (un nivel)          | P2 / 3     | Después de on-call básico                                         |

## Cómo se marca el avance

El avance se registra en la hoja, columna Estado (Pendiente · En progreso · Hecho · Descartado):
al fusionar el PR que cierra una feature, se pasa a Hecho y la pestaña Resumen recalcula el
porcentaje por tier. Los PRs mencionan el ID de la hoja en la descripción (por ejemplo "Cierra
F-01 y F-27") para poder rastrearlos.
