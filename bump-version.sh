#!/usr/bin/env bash
# Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
# No use to design, develop, specify, test, validate, or improve Other Implementations,
# subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
# Text and data mining rights, including for AI training, are reserved under
# Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

#
# Bump the crate version across every file that hard-codes it, then refresh
# Cargo.lock. The current version is read from rodbus/Cargo.toml, so you only
# pass the new one.
#
#   ./bump-version.sh 1.6.0-RC1
#
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <new-version>   (e.g. 1.6.0-RC1)" >&2
  exit 1
fi

NEW="$1"
# X.Y.Z with an optional pre-release suffix; no build metadata, whose '+' the matching below can't handle
if ! [[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "not a version: $NEW (expected X.Y.Z or X.Y.Z-PRERELEASE)" >&2
  exit 1
fi
cd "$(dirname "$0")"

# The files that hard-code the version. This list is the source of truth. The examples,
# integration tests and rodbus-client are never published and stay at 0.1.0.
FILES=(
  rodbus/Cargo.toml
  ffi/rodbus-ffi/Cargo.toml
  ffi/rodbus-ffi-c/Cargo.toml
  ffi/rodbus-ffi-java/Cargo.toml
  ffi/rodbus-schema/Cargo.toml
  ffi/rodbus-bindings/Cargo.toml
  ffi/bindings/java/examples/pom.xml
  ffi/bindings/java/rodbus-tests/pom.xml
  ffi/bindings/dotnet/examples/client/client.csproj
  ffi/bindings/dotnet/examples/server/server.csproj
  ffi/bindings/c/CMakeLists.txt
  guide/sitedata.json
  rodbus/README.md
  SECURITY.md
)

OLD="$(sed -n 's/^version = "\(.*\)"/\1/p' rodbus/Cargo.toml | head -n1)"
if [ -z "$OLD" ]; then
  echo "could not determine current version from rodbus/Cargo.toml" >&2
  exit 1
fi

if [ "$OLD" = "$NEW" ]; then
  echo "current version is already $NEW; nothing to do" >&2
  exit 1
fi

# Match the version only as a whole token, so 1.6.0-RC1 isn't found inside 1.6.0-RC10
OLD_RE="(^|[^0-9A-Za-z.-])${OLD//./\\.}([^0-9A-Za-z.-]|$)"

echo "bumping $OLD -> $NEW"
for f in "${FILES[@]}"; do
  if ! grep -qE -- "$OLD_RE" "$f"; then
    echo "  warning: $OLD not found in $f" >&2
    continue
  fi
  # Repeat: two versions one separator apart share it, so one pass can't replace both
  while grep -qE -- "$OLD_RE" "$f"; do
    sed -i -E "s/${OLD_RE}/\\1${NEW}\\2/g" "$f"
  done
  echo "  updated $f"
done

echo "refreshing Cargo.lock"
cargo update -p rodbus -p rodbus-ffi -p rodbus-ffi-c -p rodbus-ffi-java -p rodbus-schema -p rodbus-bindings --precise "$NEW"

echo "verifying no '$OLD' references remain in the bumped files"
stray=()
for f in "${FILES[@]}"; do
  if grep -qE -- "$OLD_RE" "$f"; then
    stray+=("$f")
  fi
done
if [ ${#stray[@]} -ne 0 ]; then
  echo "  version string still present in: ${stray[*]}" >&2
  exit 1
fi
echo "done. review 'git diff', then update CHANGELOG.md."
