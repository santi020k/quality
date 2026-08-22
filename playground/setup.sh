#!/bin/sh

set -eu

playground_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
target=${1:-"$playground_dir/.sandbox"}

if [ -e "$target" ]; then
  echo "Playground already exists at $target" >&2
  echo "Remove it explicitly if you want to start over." >&2
  exit 1
fi

mkdir -p "$target/tools"
cp -R "$playground_dir/fixture/." "$target/"
cp "$playground_dir/tools/demo-tool" "$target/tools/demo-tool"
chmod +x "$target/tools/demo-tool"
printf 'Hello from the formatter playground.  \n' > "$target/src/message.prose"

git -C "$target" init --quiet
git -C "$target" config user.email "quality-playground@example.test"
git -C "$target" config user.name "Quality Playground"
git -C "$target" add .
git -C "$target" commit --quiet -m "Initial playground fixture"

echo "Created quality playground at $target"
echo "Next: pnpm playground -- doctor"
