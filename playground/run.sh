#!/bin/sh

set -eu

playground_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_dir=$(CDPATH= cd -- "$playground_dir/.." && pwd)
sandbox="$playground_dir/.sandbox"

if [ ! -d "$sandbox/.git" ]; then
  sh "$playground_dir/setup.sh" "$sandbox"
fi

if [ "${1:-}" = "--" ]; then
  shift
fi

if [ "$#" -eq 0 ]; then
  set -- doctor
fi

exec cargo run --quiet --manifest-path "$repository_dir/Cargo.toml" -- \
  --root "$sandbox" "$@"
