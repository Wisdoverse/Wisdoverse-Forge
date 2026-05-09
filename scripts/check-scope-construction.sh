#!/bin/sh
set -eu

repo_root="${SCOPE_CONSTRUCTION_REPO_ROOT:-$(pwd)}"
scan_root="${SCOPE_CONSTRUCTION_SCAN_ROOT:-$repo_root/rust}"
allowlist_file="${SCOPE_CONSTRUCTION_ALLOWLIST:-$repo_root/rust/crates/auth/src/scope_construction_allowlist.txt}"

if [ ! -d "$scan_root" ]; then
  echo "ERROR: scope construction scan root does not exist: $scan_root" >&2
  exit 2
fi

if [ ! -f "$allowlist_file" ]; then
  echo "ERROR: scope construction allowlist not found: $allowlist_file" >&2
  exit 2
fi

allowed="$(mktemp)"
violations="$(mktemp)"
trap 'rm -f "$allowed" "$violations"' EXIT

sed -e 's/[[:space:]]*#.*$//' -e '/^[[:space:]]*$/d' "$allowlist_file" > "$allowed"

is_allowed() {
  rel_path="$1"
  grep -Fxq "$rel_path" "$allowed"
}

find "$scan_root" -type f -name '*.rs' | while IFS= read -r file; do
  rel_path="${file#$repo_root/}"
  if is_allowed "$rel_path"; then
    continue
  fi

  grep -nE 'TenantScope::new[[:space:]]*\(|Scoped(Read|Write)::unchecked_[[:alnum:]_]*[[:space:]]*\(' "$file" \
    | sed "s|^|$rel_path:|" >> "$violations" || true
done

if [ -s "$violations" ]; then
  cat >&2 <<'MSG'
ERROR: direct scope construction is restricted.

Move test construction through rust/crates/api/src/test_support.rs, or add a
documented allowlist entry only when the constructor itself is being tested.

Violations:
MSG
  cat "$violations" >&2
  exit 1
fi

echo "scope construction guard passed"
