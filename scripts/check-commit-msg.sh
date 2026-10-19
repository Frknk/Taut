#!/usr/bin/env bash
# Valida que el mensaje de commit siga Conventional Commits.
# Uso: scripts/check-commit-msg.sh <archivo-con-el-mensaje>   (lefthook lo llama en commit-msg)
set -euo pipefail

file="${1:?uso: $0 <archivo del mensaje>}"
subject="$(grep -v '^#' "$file" | sed -n '1p')"

types='feat|fix|docs|chore|ci|build|refactor|test|perf|revert'
pattern="^(${types})(\([a-z0-9._/-]+\))?!?: [^ ].{0,71}$"

# Merges, reverts generados por git y fixups pasan sin validar.
if [[ "$subject" =~ ^(Merge|Revert\ \"|fixup!|squash!) ]]; then
  exit 0
fi

if [[ ! "$subject" =~ $pattern ]]; then
  cat >&2 <<MSG
✗ Mensaje de commit inválido:
    $subject

  Formato (Conventional Commits):
    <tipo>(<scope opcional>): <descripción en inglés, imperativo, máx. 72 caracteres>
  Tipos:            ${types//|/, }
  Scopes sugeridos: api, web, db, http, config, telemetry, infra, loadtest, ci, docs
  Ejemplos:         feat(api): add monitor creation endpoint
                    fix(web): handle missing PUBLIC_API_URL
                    chore: bump sqlx to 0.9.1

  Saltar solo en emergencias: LEFTHOOK=0 git commit ...
MSG
  exit 1
fi
