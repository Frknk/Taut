# loadtest (semana 6)

Escenarios de k6 versionados junto al código (`just loadtest`), exportando a Prometheus:

1. `api-crud.js`: 50 usuarios virtuales creando y listando monitores, 5 min. Umbral: p95 < 300 ms,
   errores < 1 %.
2. `status-page.js`: 500 VUs leyendo páginas de estado públicas. Se corre sin caché (sem. 6) y con
   Redis (sem. 13); el delta es el entregable.
3. `checker-soak.js`: 2 000 monitores sintéticos a 30 s contra un servidor de eco interno, 30 min.
   Verifica que `uptime_check_lag_seconds` se mantenga bajo 10 s y que el HPA escale.

Un job nightly correrá k6 contra staging y fallará si p95 > 300 ms.
