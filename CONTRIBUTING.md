# Cómo contribuir a taut

## Flujo de trabajo

1. Crea una rama desde `main` con el tipo y un nombre corto: `feat/monitor-crud`,
   `fix/ready-timeout`, `docs/adr-cloud-provider`.
2. Commits pequeños con mensajes en formato Conventional Commits (abajo). Los hooks de lefthook
   validan formato, lint y mensaje antes de que el commit exista.
3. `just check` en verde antes de abrir el PR: es lo mismo que corre CI.
4. Abre el PR contra `main` usando la plantilla (qué cambia, cómo probarlo, checklist). Si cambia
   una decisión registrada, el ADR nuevo va en el mismo PR.
5. Se integra con *squash merge*; el título del PR es el mensaje del commit final, así que también
   debe seguir el formato.

La protección de la rama `main` (PR obligatorio, CI en verde) no está disponible en repositorios
privados del plan gratuito de GitHub; se activa en cuanto el repositorio sea público. Mientras
tanto la regla es de equipo: nada entra a `main` sin PR y sin CI en verde.

## Mensajes de commit

Formato: `<tipo>(<scope opcional>): <descripción en inglés, imperativo, máx. 72 caracteres>`.

- Tipos: `feat`, `fix`, `docs`, `chore`, `ci`, `build`, `refactor`, `test`, `perf`, `revert`.
- Scopes sugeridos: `api`, `auth`, `identity`, `web`, `db`, `http`, `config`, `telemetry`, `infra`,
  `ci`, `docs`.
- Un `!` antes de los dos puntos marca un cambio incompatible: `feat(api)!: drop v0 routes`.

Válidos:

```
feat(api): add monitor creation endpoint
fix(web): handle missing PUBLIC_API_URL
chore: bump sqlx to 0.9.1
```

Inválidos:

```
Feat: capital letter in type        → el tipo va en minúsculas
agregado endpoint de monitores      → falta el tipo y va en español
```

## Hooks de git (lefthook)

| Hook         | Qué corre                                                                 |
| ------------ | ------------------------------------------------------------------------- |
| `pre-commit` | `cargo fmt --check`; `oxlint` y `oxfmt --check` sobre los archivos staged de `web/` |
| `commit-msg` | `scripts/check-commit-msg.sh`: valida el formato del mensaje              |
| `pre-push`   | `cargo clippy -D warnings` y `cargo test --lib` (tests unitarios)         |

`just setup` los instala (`lefthook install`). Saltarlos solo en una emergencia real:
`LEFTHOOK=0 git commit ...`; CI vuelve a correr todo igual.

## Antes de abrir el PR

- `just check` en verde (formato, clippy, oxlint, svelte-check, tests con la base de datos
  levantada, cargo-deny, build del frontend).
- Si tocaste queries de sqlx, `just sqlx-prepare` y versiona `.sqlx/`.
- Si agregaste una migración, es compatible hacia atrás (expand → migrate → contract).
- Documentación al día: README si cambia el uso, ADR si cambia una decisión, `AGENTS.md` si cambia
  una convención.

## Licencia de las contribuciones

El repositorio está bajo la licencia [MIT](LICENSE). Al abrir un PR aceptas que tu contribución se
publique bajo esa misma licencia, con el aviso de copyright del repositorio.
