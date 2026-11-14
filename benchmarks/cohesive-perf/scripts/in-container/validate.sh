# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code): input
# validation shared by the host scripts (macOS bash 3.2, Linux bash 5) and the
# in-container scripts (sourced, not executed). Checks: every public
# entry point validates its identifiers, counts and enums before any path is
# built, and destructive operations are confined to the harness roots.
#
# Each perf_check_* function prints an error and returns 2 on invalid input;
# callers use `perf_check_x ... || exit 2` (the functions never exit
# themselves, so they can be unit-tested: scripts/tests/test_validate.sh).

perf_err() { echo "ERROR: $*" >&2; return 2; }

# Campaign identifier: one path component, [A-Za-z0-9][A-Za-z0-9._-]{0,63},
# no "..": no absolute path, traversal, shell or sed metacharacters.
perf_check_campaign() {
  local re='^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$'
  [[ "$1" =~ $re ]] && [[ "$1" != *..* ]] \
    || perf_err "invalid campaign '$1' (one component, [A-Za-z0-9][A-Za-z0-9._-]{0,63}, no '..')"
}

# Positive integer (no sign, no leading zero, < 1e9): perf_check_posint NAME VALUE
perf_check_posint() {
  local re='^[1-9][0-9]{0,8}$'
  [[ "$2" =~ $re ]] || perf_err "$1 must be a positive integer, got '$2'"
}

# Non-negative integer (< 1e9): perf_check_uint NAME VALUE
perf_check_uint() {
  local re='^(0|[1-9][0-9]{0,8})$'
  [[ "$2" =~ $re ]] || perf_err "$1 must be a non-negative integer, got '$2'"
}

# Space-separated list of positive integers, at least one: perf_check_posint_list NAME LIST
perf_check_posint_list() {
  local name="$1" v n=0
  for v in $2; do
    perf_check_posint "${name} entry" "${v}" || return 2
    n=$((n + 1))
  done
  [ "${n}" -gt 0 ] || perf_err "${name} must list at least one positive integer"
}

# Mesh size h: a positive decimal (0.12, 0.06, 1, 1.5); used in file names.
perf_check_h() {
  local re='^(0\.[0-9]{0,5}[1-9]|[1-9][0-9]{0,2}(\.[0-9]{0,5}[1-9])?)$'
  [[ "$1" =~ $re ]] || perf_err "h must be a positive decimal like 0.12 (no trailing zeros), got '$1'"
}

# Enumerations: perf_check_enum NAME VALUE ALLOWED...
perf_check_enum() {
  local name="$1" value="$2" a
  shift 2
  for a in "$@"; do
    [ "${value}" != "${a}" ] || return 0
  done
  perf_err "${name} must be one of: $*; got '${value}'"
}

# Space-separated list of enumeration values, at least one:
# perf_check_enum_list NAME LIST ALLOWED...
perf_check_enum_list() {
  local name="$1" list="$2" v n=0
  shift 2
  for v in ${list}; do
    perf_check_enum "${name} entry" "${v}" "$@" || return 2
    n=$((n + 1))
  done
  [ "${n}" -gt 0 ] || perf_err "${name} must list at least one of: $*"
}

perf_check_config() { perf_check_enum config "$1" default strict; }
perf_check_material() { perf_check_enum material "$1" elastic cohesive; }
perf_check_fnte() { perf_check_enum fnte "$1" upstream joss; }

# Docker memory limit: integer with optional unit b/k/m/g.
perf_check_memory() {
  local re='^([1-9][0-9]{0,6}[bkmg]|[1-9][0-9]{0,12})$'
  [[ "$1" =~ $re ]] || perf_err "memory must look like 8g / 14g / 512m, got '$1'"
}

# Mesh path inside the scratch volume: /perf/meshes/<name>.msh
perf_check_mesh_path() {
  local re='^/perf/meshes/[A-Za-z0-9][A-Za-z0-9._-]{0,63}[.]msh$'
  [[ "$1" =~ $re ]] && [[ "$1" != *..* ]] || perf_err "invalid mesh path '$1' (/perf/meshes/<name>.msh)"
}

# Extra user-section parameter key=value. Keys are identifiers; values are
# plain tokens (no '|', '/', '\', '&', quotes, spaces: they end up in a sed
# expression and in input.dat). mesh and max_steps are set by the harness.
perf_check_kv() {
  local re='^[a-z][a-z0-9_]{0,63}=[A-Za-z0-9._+-]{1,64}$'
  [[ "$1" =~ $re ]] \
    || { perf_err "invalid parameter '$1' (key=value, key [a-z][a-z0-9_]*, value [A-Za-z0-9._+-]+)"; return 2; }
  case "${1%%=*}" in
    mesh|max_steps) perf_err "parameter '${1%%=*}' is set by the harness, not by key=value"; return 2 ;;
  esac
  return 0
}

# Run directory: <root>/<campaign>/<component>.../rep<k>, every component
# [A-Za-z0-9][A-Za-z0-9._-]*, no "..", at least <campaign>/<x>/rep<k>.
# perf_check_run_dir ROOT PATH (lexical check; see perf_confine_dir for the
# symlink-resolving check done before deleting anything).
perf_check_run_dir() {
  _perf_check_dir_under "$1" "$2" '' "<campaign>/.../rep<k>"
}

# Lexical check shared by perf_check_run_dir / perf_check_attempt_dir: PATH
# must be ROOT/ followed by components matching the pattern (ROOT is
# compared as a plain string prefix, never as a regular expression).
_perf_check_dir_under() {
  local root="$1" path="$2" suffix="$3" what="$4" rel
  local comp='[A-Za-z0-9][A-Za-z0-9._-]{0,63}'
  local re="^${comp}(/${comp})+/rep[1-9][0-9]{0,3}${suffix}\$"
  case "${path}" in
    "${root}"/*) rel="${path#"${root}"/}" ;;
    *) perf_err "directory '${path}' is not under '${root}/'"; return 2 ;;
  esac
  [[ "${rel}" =~ ${re} ]] && [[ "${rel}" != *..* ]] \
    || perf_err "directory '${path}' is not ${root}/${what}"
}

# Same for an invalid-attempt directory (rep<k>.invalid-<n>) or a run
# directory: used by rerun-invalid.sh before moving a run aside.
perf_check_attempt_dir() {
  _perf_check_dir_under "$1" "$2" '(\.invalid-[1-9][0-9]{0,3})?' "<campaign>/.../rep<k>[.invalid-<n>]"
}

# Resolve symlinks of PATH (which need not exist) and require that it is
# strictly inside ROOT (itself resolved) and unchanged by the resolution (no
# symlinked component): perf_confine_dir ROOT PATH. Uses python3 (present in
# the oracle image and on every host this harness supports).
perf_confine_dir() {
  python3 -I - "$1" "$2" <<'PY' || perf_err "'$2' is not confined to '$1' (or contains a symlink)"
import os, sys
root, path = sys.argv[1], sys.argv[2]
real_root = os.path.realpath(root)
real = os.path.realpath(path)
ok = (os.path.isabs(path) and os.path.normpath(path) == path
      and real == os.path.join(real_root, os.path.relpath(path, root))
      and os.path.commonpath([real_root, real]) == real_root and real != real_root)
sys.exit(0 if ok else 1)
PY
}
