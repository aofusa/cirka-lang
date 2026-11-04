#!/usr/bin/env bash
# Loads .env (or .env.sample when .env does not exist) and exports CIRKA_*.
# Usage: source scripts/env.sh
# Variables that are already set in the environment take precedence.
_cirka_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_cirka_env="$_cirka_root/.env"
[ -f "$_cirka_env" ] || _cirka_env="$_cirka_root/.env.sample"
while IFS= read -r _line || [ -n "$_line" ]; do
  _line="${_line#"${_line%%[![:space:]]*}"}"
  case "$_line" in ''|'#'*) continue ;; esac
  _line="${_line#export }"
  _key="${_line%%=*}"
  _val="${_line#*=}"
  case "$_key" in *[!A-Za-z0-9_]*|'') continue ;; esac
  if [ -z "${!_key+x}" ]; then
    export "$_key=$_val"
  fi
done < "$_cirka_env"
export CIRKA_ROOT="$_cirka_root"
unset _cirka_root _cirka_env _line _key _val
