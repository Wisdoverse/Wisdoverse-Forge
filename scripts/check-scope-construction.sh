#!/bin/sh
set -eu

# Defense-in-depth guard (ADR 0004): a TenantScope (or an unchecked Scoped
# read/write) may only be constructed at sanctioned production sites:
#   - the auth middleware and the WebSocket gateway, allowlisted per FUNCTION
#     (`path::fn_name`), so a new wrapper function in those files is flagged;
#   - background work, through `TenantScope::for_background`, which this
#     scanner does not flag.
# It also rejects the cheap evasions: a renamed import or type alias of
# TenantScope, whitespace or a line break inside `TenantScope::new`, a function
# pointer (`.map(TenantScope::new)`), and any `impl` block for TenantScope
# outside its definition (where `Self::new` would be reachable).
# Known limit: this is a text scan, not a type check; a macro that pastes the
# identifier together would not be seen. Review covers that.
#
# Test constructors are NOT flagged: this scanner skips `/tests/` integration
# directories and `#[cfg(test)]` / test-support `#[cfg]` blocks, so the
# allowlist need only cover genuine PRODUCTION construction sites.

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

# An entry is either a whole file (`path`) or one function (`path::fn_name`).
is_allowed() {
  grep -Fxq "$1" "$allowed"
}

# Restricted constructions, matched on one production line at a time.
pattern='TenantScope[[:space:]]*>?[[:space:]]*::[[:space:]]*(new|with_axes)([^[:alnum:]_]|$)'
pattern="$pattern"'|Scoped(Read|Write)[[:space:]]*::[[:space:]]*unchecked_'
pattern="$pattern"'|TenantScope[[:space:]]+as[[:space:]]'
pattern="$pattern"'|(^|[^[:alnum:]_])type[[:space:]]+[[:alnum:]_]+[^=]*=[^;]*TenantScope[[:space:]]*;'
# The impl rule is anchored on the TAB that starts the source text in the
# `LINENO<TAB>FN<TAB>line` records below (an impl item starts its line).
tab="$(printf '\t')"
pattern="$pattern"'|'"$tab"'[[:space:]]*(unsafe[[:space:]]+)?impl([^[:alnum:]_][^{;]*)?[^[:alnum:]_]TenantScope([^[:alnum:]_][^{;]*)?(\{|$)'

# Emit only PRODUCTION lines of a Rust file as `LINENO<TAB>FN<TAB>line`, where FN
# is the innermost enclosing function (`-` at item level). Skips comment lines
# and `#[cfg(test)]` / test-support `#[cfg(...)]` blocks by tracking brace
# depth, mirroring the production-line logic in route_ddd_boundary_test.rs.
# A line ending in `TenantScope`, `TenantScope::` or `::` is joined with the
# next one so a line break cannot split a constructor path.
production_lines() {
  awk '
    function braces(s,   o, c) { o = gsub(/{/, "{", s); c = gsub(/}/, "}", s); return o - c }
    function current_fn() { return sp > 0 ? fn_name[sp] : "-" }
    BEGIN { skip = 0; pending = 0; depth = 0; sp = 0; pending_fn = ""; held = ""; held_no = 0; held_fn = "" }
    {
      if (skip > 0) {
        skip += braces($0)
        if (skip <= 0) { skip = 0 }
        next
      }
      t = $0
      sub(/^[ \t]+/, "", t)
      if (t ~ /^\/\//) { next }
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

      if (match($0, /(^|[^[:alnum:]_])fn[ \t]+[[:alnum:]_]+/)) {
        name = substr($0, RSTART, RLENGTH)
        sub(/^.*fn[ \t]+/, "", name)
        pending_fn = name
      }
      if (pending_fn != "" && index($0, "{") > 0) {
        sp++; fn_name[sp] = pending_fn; fn_depth[sp] = depth; pending_fn = ""
      } else if (pending_fn != "" && t ~ /;[ \t]*$/) {
        pending_fn = ""
      }

      if (held != "") {
        line = held " " t; no = held_no; f = held_fn; held = ""
      } else {
        line = $0; no = FNR; f = current_fn()
      }
      if (line ~ /(TenantScope|::|>)[ \t]*$/) {
        held = line; held_no = no; held_fn = f
      } else {
        print no "\t" f "\t" line
      }

      depth += braces($0)
      while (sp > 0 && depth <= fn_depth[sp]) { sp-- }
    }
    END { if (held != "") { print held_no "\t" held_fn "\t" held } }
  ' "$1"
}

find "$scan_root" -type f -name '*.rs' | while IFS= read -r file; do
  rel_path="${file#"$repo_root"/}"

  # Integration test directories are entirely test code.
  case "$rel_path" in
    */tests/*) continue ;;
  esac

  if is_allowed "$rel_path"; then
    continue
  fi

  production_lines "$file" | grep -E "$pattern" | while IFS="$(printf '\t')" read -r line_no fn_name text; do
    if is_allowed "$rel_path::$fn_name"; then
      continue
    fi
    printf '%s:%s: [fn %s] %s\n' "$rel_path" "$line_no" "$fn_name" "$text" >> "$violations"
  done || true
done

if [ -s "$violations" ]; then
  cat >&2 <<'MSG'
ERROR: direct scope construction is restricted.

TenantScope::new / TenantScope::with_axes are reserved for the auth middleware
and the WebSocket gateway. Background work must use
TenantScope::for_background(org_id, acting_user, workspace_id) with the
organization taken from a loaded database row (see ADR 0004). Do not alias,
re-implement or wrap the constructors. Tests should go through
rust/crates/api/src/test_support.rs.

Violations:
MSG
  cat "$violations" >&2
  exit 1
fi

echo "scope construction guard passed"
