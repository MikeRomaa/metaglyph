#!/usr/bin/env bash
# Parity check between the canonical rowan parser (mg-syntax, via `mg check`)
# and the Tree-sitter grammar (plan 4, Z1).
#
# Rule (plan 4, Z1): a file with no MG01xx (syntax) diagnostic from `mg`
# must parse with no ERROR/MISSING node in Tree-sitter. The reverse does
# not hold -- Tree-sitter is deliberately looser, so it may accept files
# the compiler rejects for semantic reasons.
#
# Run from Git Bash, from anywhere:
#   editors/tree-sitter/script/parity-check.sh

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
grammar_dir="$(cd "$script_dir/.." && pwd)"
repo_root="$(cd "$grammar_dir/../.." && pwd)"

ts_bin="$grammar_dir/node_modules/tree-sitter-cli/tree-sitter.exe"
if [[ ! -x "$ts_bin" ]]; then
  ts_bin="$grammar_dir/node_modules/.bin/tree-sitter"
fi

mg_bin="$repo_root/target/debug/mg"
[[ -x "$mg_bin" ]] || mg_bin="$repo_root/target/debug/mg.exe"
if [[ ! -x "$mg_bin" ]]; then
  echo "building mg-cli..." >&2
  (cd "$repo_root" && cargo build -q -p mg-cli)
fi

files=("$repo_root"/samples/*.mg "$repo_root"/crates/mg-syntax/tests/diagnostics/*.mg)

ts_out="$(mktemp)"
trap 'rm -f "$ts_out"' EXIT

fail=0
checked=0
for file in "${files[@]}"; do
  [[ -f "$file" ]] || continue
  checked=$((checked + 1))

  mg_stderr="$("$mg_bin" check "$file" 2>&1 >/dev/null || true)"
  has_syntax_error=0
  if grep -q 'MG01[0-9][0-9]' <<<"$mg_stderr"; then
    has_syntax_error=1
  fi

  if (cd "$grammar_dir" && "$ts_bin" parse "$file" --quiet) >"$ts_out" 2>&1; then
    ts_clean=1
  else
    ts_clean=0
  fi

  if [[ $has_syntax_error -eq 0 && $ts_clean -eq 0 ]]; then
    echo "PARITY VIOLATION: $file"
    echo "  mg check: no MG01xx diagnostic"
    echo "  tree-sitter: ERROR/MISSING node present"
    sed 's/^/  ts> /' "$ts_out"
    fail=1
  fi
done

echo "checked $checked files"
if [[ $fail -ne 0 ]]; then
  echo "parity check FAILED"
  exit 1
fi
echo "parity check passed"
