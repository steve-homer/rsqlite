#!/usr/bin/env bash
# Download the larger real-world sample databases into tests/fixtures/external/.
# They are too big to commit (25 MB / 6 MB), so they are gitignored and the tests
# that use them skip when the files are absent. URLs are pinned to commits and
# verified by SHA-256.
set -euo pipefail

dest="$(cd "$(dirname "$0")/.." && pwd)/tests/fixtures/external"
mkdir -p "$dest"

fetch() {
  local name=$1 url=$2 sha=$3
  local out="$dest/$name"
  if [[ -f $out ]] && echo "$sha  $out" | sha256sum -c --status; then
    echo "ok       $name"
    return
  fi
  echo "fetching $name"
  curl -fsSL -o "$out.part" "$url"
  if ! echo "$sha  $out.part" | sha256sum -c --status; then
    echo "checksum mismatch for $name" >&2
    rm -f "$out.part"
    exit 1
  fi
  mv "$out.part" "$out"
}

# Northwind (MIT, JP White): 13 tables + 17 views, spaces in names
# ("Order Details"), BLOB pictures, ~609k rows in the largest table.
fetch northwind.db \
  https://github.com/jpwhite3/northwind-SQLite3/raw/4f56e7f5906dfd23b25244c5bfe8fb5da6402efd/dist/northwind.db \
  2f4f5c68dfcd33ba27373eae48c7a4869800c68095ee0f9f0da494f83382a877

# Sakila (BSD-3-Clause, Bradley Grant): 16 tables + 5 views, written by
# SQLite 3.11 with a 1024-byte page size, timestamps, triggers.
fetch sakila.db \
  https://github.com/bradleygrant/sakila-sqlite3/raw/9394b42d13888c3d3d3d56cd7e9c84fadafb71c7/sakila_master.db \
  88c91a4a1a6b61f9d3f35904c0a173c887b25e73f20c3c2fdb073818c06f4268
