#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "Error: jq is required (install with your package manager)." >&2
  exit 1
fi

DOCS_JSON="docs/docs.json"

nav_pages=$(mktemp)
fs_pages=$(mktemp)
version_refs=$(mktemp)
cleanup() {
  rm -f "$nav_pages" "$fs_pages" "$version_refs"
}
trap cleanup EXIT

jq -r '.navigation.groups[].pages[]' "$DOCS_JSON" | sort | grep -v '^README$' > "$nav_pages"
{
  # Individual authentication PRDs are archived evidence. Publish their index
  # only, so obsolete multi-crate guidance cannot silently re-enter nav.
  find docs -type f -name '*.md' \
    | grep -v '^docs/README\.md$' \
    | grep -v '^docs/prd/' \
    | grep -v '^docs/plans/'
  printf '%s\n' docs/prd/README.md
} | sed 's#^docs/##' \
  | sed 's#\.md$##' \
  | sort > "$fs_pages"

if ! diff -u "$nav_pages" "$fs_pages" > /tmp/verify-docs.diff; then
  echo "Documentation navigation mismatch detected."
  echo "Pages present in docs/ folder but missing in docs/docs.json:" >&2
  comm -23 "$fs_pages" "$nav_pages" >&2
  echo "Pages in docs/docs.json but not present on disk:" >&2
  comm -13 "$fs_pages" "$nav_pages" >&2
  echo "" >&2
  echo "See /tmp/verify-docs.diff for exact diff." >&2
  exit 1
fi

crate_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
if [ -z "$crate_version" ]; then
  echo "Unable to determine root crate version from Cargo.toml." >&2
  exit 1
fi

{
  printf '%s\n' README.md
  find docs -type f -name '*.md' | sort
} | while IFS= read -r file; do
  grep -nE 'ddd_cqrs_es = (\{ version = )?"[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?"' "$file" \
    | sed "s#^#$file:#" || true
done > "$version_refs"

if mismatches=$(awk -v version="$crate_version" 'index($0, "\"" version "\"") == 0 { print; bad = 1 } END { exit bad ? 0 : 1 }' "$version_refs"); then
  echo "Documentation crate version mismatch detected." >&2
  echo "Expected ddd_cqrs_es install snippets to use version $crate_version." >&2
  echo "$mismatches" >&2
  exit 1
fi

echo "docs.json navigation, docs/**/*.md, and ddd_cqrs_es install snippets are aligned."
