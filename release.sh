#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 0 || ${1:-} == --help || ${1:-} == -h ]]; then
    printf 'Usage: %s vMAJOR.MINOR.PATCH [--build-only] [--inno-setup PATH]\n' "$0"
    if [[ $# -eq 0 ]]; then exit 2; fi
    exit 0
fi

tag=$1
shift
script_path=${BASH_SOURCE[0]//\\//}
if [[ $script_path == */* ]]; then script_dir=${script_path%/*}; else script_dir=.; fi
script_dir="$(CDPATH= cd -- "$script_dir" && pwd)"
exec node "$script_dir/scripts/release-local.mjs" --tag "$tag" "$@"
