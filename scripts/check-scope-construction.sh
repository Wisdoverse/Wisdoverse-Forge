#!/bin/sh
set -eu

# Defense-in-depth guard: a TenantScope (or an unchecked Scoped read/write) may
# only be constructed at sanctioned production sites (auth middleware / gateway /
# documented background workers). Test constructors are NOT flagged: this scanner
# skips `/tests/` integration directories and `#[cfg(test)]` / test-support `#[cfg]`
# blocks, so the allowlist need only cover genuine PRODUCTION construction sites
# (a file-level allowlist would otherwise blind the guard to new production
# constructors in any module that also has inline tests — codex review).

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

# Emit only PRODUCTION lines of a Rust file, as `LINENO:line`. Skips
# `#[cfg(test)]` / test-support `#[cfg(...)]` blocks by tracking brace depth,
# mirroring the production-line logic in route_ddd_boundary_test.rs.
production_lines() {
  awk '
    function braces(s,   o, c) { o = gsub(/{/, "{", s); c = gsub(/}/, "}", s); return o - c }
    BEGIN { skip = 0; pending = 0 }
    {
      if (skip > 0) {
        skip += braces($0)
        if (skip <= 0) { skip = 0 }
        next
      }
      t = $0
      sub(/^[ \t]+/, "", t)
      if (pending) {
        # Consume the cfg-gated item header, which may span multiple lines
        # (e.g. a multiline `fn helper(\n ...\n) -> TenantScope {`). Stay in
        # `pending` until the item either opens a `{` block (then skip until its
        # braces balance) or terminates as a one-line `;` declaration.
        d = braces($0)
        if (index($0, "{") > 0 || d > 0) {
          pending = 0
          if (d > 0) { skip = d }
          next
        }
        if (t ~ /;[ \t]*$/) { pending = 0 }
        next
      }
      if (t ~ /^#\[cfg\(test\)\]/ \
          || t ~ /^#\[cfg\(feature = "test-support"\)\]/ \
          || t ~ /^#\[cfg\(any\(test, feature = "test-support"\)\)\]/) {
        pending = 1
        next
      }
      print FNR ":" $0
    }
  ' "$1"
}

find "$scan_root" -type f -name '*.rs' | while IFS= read -r file; do
  rel_path="${file#$repo_root/}"

  # Integration test directories are entirely test code.
  case "$rel_path" in
    */tests/*) continue ;;
  esac

  if is_allowed "$rel_path"; then
    continue
  fi

  production_lines "$file" \
    | grep -E 'TenantScope::(new|with_axes)[[:space:]]*\(|Scoped(Read|Write)::unchecked_[[:alnum:]_]*[[:space:]]*\(' \
    | sed "s|^|$rel_path:|" >> "$violations" || true
done

if [ -s "$violations" ]; then
  cat >&2 <<'MSG'
ERROR: direct scope construction is restricted.

Construct TenantScope only in auth middleware / gateway / a documented worker,
move test construction through rust/crates/api/src/test_support.rs, or add a
documented allowlist entry only when the constructor itself is being tested.

Violations:
MSG
  cat "$violations" >&2
  exit 1
fi

echo "scope construction guard passed"
