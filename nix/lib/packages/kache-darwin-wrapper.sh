#!/usr/bin/env bash
# Linked Darwin outputs have not passed compile-and-compare qualification.
# Preserve the complete rustc command for them. Cache only Rust libraries.
set -euo pipefail
arguments=("$@")
types=()
for ((i=1; i<${#arguments[@]}; i++)); do
  case "${arguments[$i]}" in
    --test) exec "${arguments[@]}" ;;
    --crate-type)
      i=$((i+1))
      IFS=, read -r -a current <<< "${arguments[$i]:-}"
      types+=("${current[@]}")
      ;;
    --crate-type=*)
      IFS=, read -r -a current <<< "${arguments[$i]#--crate-type=}"
      types+=("${current[@]}")
      ;;
  esac
done
if [[ ${#types[@]} -eq 0 ]]; then
  exec "${arguments[@]}"
fi
for kind in "${types[@]}"; do
  case "$kind" in
    lib|rlib) ;;
    *) exec "${arguments[@]}" ;;
  esac
done
cache="$(dirname "${BASH_SOURCE[0]}")/kache"
exec "$cache" "${arguments[@]}"
