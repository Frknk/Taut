# Reportes de carga

Un reporte por corrida de k6, escrito desde [TEMPLATE.md](TEMPLATE.md). Cómo se corren las
pruebas: [loadtest/README.md](../../loadtest/README.md). Decisiones:
[ADR 0011](../adr/0011-pruebas-de-carga-k6.md).

- `img/<fecha>-<escenario>/`: capturas de Grafana y del dashboard web de k6.
- `data/<testid>.json`: el `summary.json` de cada corrida, para comparar después. Se copia con
  `just loadtest-keep <testid>`, que le quita `setup_data` (trae los tokens del login).

| Fecha      | Escenario                              | Commit    | Veredicto                                                     | Reporte                                      |
| ---------- | -------------------------------------- | --------- | ------------------------------------------------------------- | -------------------------------------------- |
| 2026-10-19 | api-crud (línea base)                  | `92928be` | Pasa: p95 44 ms a 135 req/s                                   | [reporte](2026-10-19-api-crud.md)            |
| 2026-10-19 | status-page (línea base sin caché)     | `92928be` | Pasa: p95 6 ms a 500 req/s; techo ~950 req/s                  | [reporte](2026-10-19-status-page.md)         |
| 2026-10-19 | rate-limit                             | `92928be` | Pasa: 600 y 60 OK por ventana, `Retry-After` correcto         | [reporte](2026-10-19-rate-limit.md)          |
| 2026-10-19 | isolation (y autotest)                 | `92928be` | Pasa: 0 fugas en 9 365 comprobaciones                         | [reporte](2026-10-19-isolation.md)           |
| 2026-10-19 | status-page stress, spike y breakpoint | `92928be` | SLO hasta ~750–900 req/s, techo ~920; se recupera en segundos | [reporte](2026-10-19-status-page-limites.md) |
