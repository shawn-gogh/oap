#!/usr/bin/env bash
# Downloads the official Dify Community Edition docker/ directory at a pinned
# version. It is fetched rather than vendored on purpose: Dify's compose is
# ~1300 lines and its nginx service bind-mounts config templates from that same
# directory, so a partial copy would either drift from upstream or quietly stop
# being the real stack — which is exactly what this lab exists to avoid.
set -euo pipefail

version="${DIFY_VERSION:-1.16.0}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
target="$here/upstream"

if [[ -d "$target" && -z "${DIFY_FETCH_FORCE:-}" ]]; then
  printf 'Dify %s already present at %s (set DIFY_FETCH_FORCE=1 to re-download)\n' \
    "$version" "$target"
  exit 0
fi

rm -rf "$target"
mkdir -p "$target"
printf 'Downloading Dify %s docker/ …\n' "$version"
curl -fsSL "https://github.com/langgenius/dify/archive/refs/tags/${version}.tar.gz" \
  | tar -xz -C "$target" --strip-components=2 "dify-${version}/docker"

# Dify ships .env.example; its compose reads .env. Never overwrite an existing
# one — it holds the SECRET_KEY that already-created accounts were built with.
if [[ ! -f "$target/.env" ]]; then
  cp "$target/.env.example" "$target/.env"
  printf 'Created %s from .env.example\n' "$target/.env"
fi

printf 'Dify %s ready at %s\n' "$version" "$target"
