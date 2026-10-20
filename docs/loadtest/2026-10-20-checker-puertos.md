# Reporte de carga: sondeo del checker, puertos y tareas (2026-10-20)

| Campo     | Valor                                                                                                                                               |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fecha     | 2026-10-20, 05:17–05:57 UTC                                                                                                                         |
| Commit    | `main` en `5f5699d` (antes); rama `perf/bus-spawn-handlers` en `2d82966` (después)                                                                  |
| testid    | antes, como el 19-10: `20261020T051728Z-checker-soak`; antes con puertos: `20261020T053221Z-checker-soak`; después: `20261020T054705Z-checker-soak` |
| k6        | 2.3.0 en contenedor; no envía carga a Taut, solo lee Prometheus cada 15 s                                                                           |
| Veredicto | El techo de ~1 000 chequeos/s del 19-10 era **agotamiento de puertos efímeros** en el contenedor del checker, no `for_each_concurrent`              |

## Resumen

El [sondeo del 19-10](2026-10-19-checker-soak.md) atribuyó el techo del checker (~1 000
chequeos/s, un núcleo de CPU) a `taut_bus::consume`, que intercala los handlers con
`for_each_concurrent` dentro de una sola tarea. Al repetirlo se vio otra cosa:

- **La mitad de los chequeos fallaba** con `connection failed: Cannot assign requested address
  (os error 99)`. El sondeo del 19-10 también: su `failed_ratio` fue de 0,50–0,58 y el reporte no
  lo mencionó. Esta vez abrió 29 393 incidentes falsos y dejó 20 078 de 30 000 monitores caídos.
- El checker abre una conexión nueva por chequeo (a propósito, para que cada resultado vea los
  problemas de DNS, TCP y TLS). Cada conexión deja el puerto 60 s en `TIME_WAIT`, y los 30 000
  monitores apuntan al mismo destino (el eco). Con el rango por defecto (32768–60999, 28 232
  puertos) un checker no puede pasar de ~470 conexiones nuevas por segundo a un mismo destino.
- Con el rango agotado, el kernel recorre el rango en cada `connect()` buscando un puerto libre:
  esa es la CPU del checker al 112 %.
- **Con los puertos sanos** (`tcp_tw_reuse=1` y el rango 1024–65535 en el contenedor del checker),
  el **mismo checker de `main` atiende 1 000 chequeos/s con el 24 % de un núcleo**, lag p99 de 1 s
  y ningún fallo.

El cambio de `taut_bus::consume` (una tarea por mensaje) no cambia la capacidad a esta carga, pero
deja la cola del checker en cero y hace que un pánico en un handler ya no termine el consumidor.

## Máquina y condiciones

- Apple M4, 16 GB, OrbStack 2.2.4 con 10 CPU y 7,8 GiB; imágenes release, configuración por
  defecto (`TAUT_CHECKS__CONCURRENCY` = 100, incidents con 32 a la vez).
- Conectado a la corriente con el modo de bajo consumo activo, con el Mac sin usar. No se compara
  en absoluto con el 19-10 (a batería).
- Cada corrida empieza con `LT_SOAK_TENANTS=3000 just loadtest-seed soak`: 3 000 tenants × 10
  monitores HTTP contra el eco, cada 30 s, es decir 1 000 chequeos/s.
- «Puertos sanos» es un override de compose solo para estas corridas (ver «Cómo reproducir»):

  ```yaml
  services:
    checker:
      sysctls:
        net.ipv4.tcp_tw_reuse: '1'
        net.ipv4.ip_local_port_range: '1024 65535'
  ```

## Resultados

| Medida (peor valor salvo indicación) | `main`, como el 19-10 (10 min) | `main`, puertos sanos (5 min) | Una tarea por mensaje, puertos sanos (5 min) |
| ------------------------------------ | -----------------------------: | ----------------------------: | -------------------------------------------: |
| Chequeos fallidos                    |                        45–57 % |                           0 % |                                          0 % |
| Chequeos con lag < 10 s              |                            0 % |                         100 % |                                        100 % |
| p99 del lag                          |                         29,8 s |                         1,0 s |                                        1,0 s |
| Expirados por segundo                |                    hasta 103,6 |                             0 |                                            0 |
| Completados / programados            |                          0,850 |                         0,997 |                                        0,997 |
| Cola del checker en NATS             |                         29 841 |                           490 |                                        **0** |
| Cola de incidents en NATS            |                          1 896 |                           316 |                                          670 |
| CPU del checker (media)              |                          112 % |                          24 % |                                         22 % |
| CPU de incidents (media)             |                           22 % |                          21 % |                                         18 % |
| CPU de PostgreSQL (media)            |                           82 % |                          88 % |                                         68 % |

- Las dos corridas con puertos sanos pasan todos los umbrales del escenario.
- La cola de incidents (670 mensajes, menos de 1 s de trabajo) queda muy por debajo del umbral de
  10 s en las dos.
- Por extrapolación (24 % de un núcleo cada 1 000 chequeos/s), el consumidor de una sola tarea
  tocaría techo cerca de 4 000 chequeos/s por réplica. Con una tarea por mensaje ese límite
  desaparece; lo siguiente que se agotaría es otra parte del pipeline (incidents o PostgreSQL),
  sin medir todavía.

## Hallazgos

| Hallazgo                                                             | Evidencia                                                                                  | Propuesta                                                                                                                                                                                                                                                            |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| El sondeo de 30 000 monitores agota los puertos efímeros del checker | `os error 99` en la mitad de los chequeos; `failed_ratio` 0,50–0,58 también el 19-10       | Para la prueba: repartir los monitores del soak entre varios destinos, o dar al checker `tcp_tw_reuse` y el rango ampliado en compose. Para Kubernetes: `net.ipv4.ip_local_port_range` es un sysctl seguro del pod; `tcp_tw_reuse` hay que habilitarlo en el kubelet |
| En producción el límite es por destino                               | El par (IP, puerto) de origen solo se repite por destino; los monitores reales van a miles | Vigilar si muchos tenants chequean el mismo destino desde una réplica (~470/s con el rango por defecto)                                                                                                                                                              |
| El umbral de fallos del soak no existe                               | `failed_ratio` es informativo, por eso el 19-10 no lo marcó                                | Agregar un umbral sobre los fallos de los monitores `lt-k-*` (no los de la base de desarrollo)                                                                                                                                                                       |
| El consumidor de una sola tarea no es el límite hoy                  | 24 % de un núcleo a 1 000 chequeos/s                                                       | La tarea por mensaje queda como margen y por el aislamiento de pánicos; medir el siguiente techo con un sondeo mayor cuando el soak tenga destinos repartidos                                                                                                        |

## Cómo reproducir

```sh
just loadtest-up
cat > /tmp/compose.sysctl.yaml <<'YAML'
services:
  checker:
    sysctls:
      net.ipv4.tcp_tw_reuse: "1"
      net.ipv4.ip_local_port_range: "1024 65535"
YAML
docker compose -f compose.yaml -f /tmp/compose.sysctl.yaml --profile app up -d --no-build checker
LT_SOAK_TENANTS=3000 just loadtest-seed soak
LT_SOAK_TENANTS=3000 caffeinate -i just loadtest checker-soak -e DURATION=5m
just loadtest-seed                                   # volver al set base
docker compose --profile app up -d --no-build checker   # el checker sin el override
```
