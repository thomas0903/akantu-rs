#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-018, CI-026, CI-027: package switches → cargo features).
# features: cargo check per feature alone, all, none, default (-D warnings), clippy + rustdoc of the feature-gated code; plan from cargo metadata
#
# The plan comes from ci/tools/feature_matrix.py (see its header). Every
# check runs with RUSTFLAGS="-D warnings": a feature combination that
# compiles only with warnings (dead code behind a cfg, unused imports) is a
# defect like any clippy warning. The plan also runs clippy and rustdoc with
# --all-features: the `lint` and `docs` tiers see default
# features only, so the mpi / scotch / mumps / cgal / python code is linted
# and documented here, in the image that has the native libraries.
#
# Native prerequisites: with the docker backend the default image is
# akantu-rs-mumps:dev (CI_FEATURES_IMAGE), which is built FROM
# akantu-rs-mpi:dev and has MPI + libclang, Scotch/PT-Scotch, MUMPS and
# Python 3 headers (the `mumps` features of akantu-linalg / akantu-solid are
# in the workspace, so the Scotch image alone leaves the mumps checks
# skipped). With the host backend they are detected (mpicc;
# $SCOTCH_INCLUDE_DIR or /usr/include/scotch/scotch.h; python3-config) or
# declared in CI_NATIVE_AVAILABLE (comma list). A check whose prerequisites
# are missing is skipped, which makes the tier incomplete, never a pass,
# unless every missing prerequisite is declared NOT APPLICABLE to this run in
# CI_NATIVE_NA (comma list, e.g. `mumps,petsc` for the second distribution
# that does not install them): the check is then `n/a` and
# ignored, so that a run on such a distribution can pass and a real failure
# stands out. Prerequisites that are neither available nor declared stay a
# skip.
#
# SECOND LEG: the mumps image has no PETSc, but the PETSc image (docker/petsc, akantu-rs-petsc:dev, FROM
# akantu-rs-mpi:dev) has MPI, libclang, Python headers, Scotch / PT-Scotch,
# MUMPS (Debian's petsc-dev depends on them) and PETSc: measured on an x86_64
# host (docs/TESTING.md), all 103 checks of the plan pass in it. With the
# docker backend and no CI_NATIVE_AVAILABLE, the checks that need `petsc` (13
# on this tree: workspace all-features and the `petsc` feature of five crates)
# are therefore not skipped in the first leg but run, with their own cargo
# target directory, in the PETSc image (CI_FEATURES_PETSC_IMAGE, default
# akantu-rs-petsc:dev; `ci/github/images.sh <cache> mpi mumps petsc` builds
# it). Image absent = a real skip = incomplete (exit 3), as before; a run that
# declares petsc not applicable (CI_NATIVE_NA, the second distribution) keeps
# the `n/a` result. legs.tsv records label, leg and image of every check.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
if [ "$CI_BACKEND" = docker ]; then
  CI_IMAGE="${CI_FEATURES_IMAGE:-akantu-rs-mumps:dev}"
  CI_TARGET_NAME="${CI_TARGET_NAME:-ci}-features"
fi
ci_tier_begin features

if ! ci_backend_ready; then
  ci_skip features "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
  ci_tier_end
  exit $?
fi
if ! ci_have python3; then
  ci_skip features "python3 is needed on the host to plan the matrix"
  ci_tier_end
  exit $?
fi

# Native prerequisites available to the backend.
if [ -n "${CI_NATIVE_AVAILABLE:-}" ]; then
  native=",$CI_NATIVE_AVAILABLE,"
elif [ "$CI_BACKEND" = docker ]; then
  case "$CI_IMAGE" in
    akantu-rs-scotch:*) native=",mpi,scotch,python," ;;
    akantu-rs-mumps:*) native=",mpi,scotch,python,mumps," ;; # FROM akantu-rs-mpi:dev + libscotch/libptscotch + MUMPS
    *) native=",mpi,python," ;;
  esac
else
  native=","
  ci_have mpicc && native="${native}mpi,"
  { [ -f "${SCOTCH_INCLUDE_DIR:-/usr/include/scotch}/scotch.h" ]; } && native="${native}scotch,"
  ci_have python3-config && native="${native}python,"
fi
echo "native_available=$native image=$CI_IMAGE backend=$CI_BACKEND" > "$(ci_file native.txt)"

# Second leg (see the header): only the docker backend with the tier's own
# native set; an explicit CI_NATIVE_AVAILABLE keeps the single-leg behaviour.
petsc_image="${CI_FEATURES_PETSC_IMAGE:-akantu-rs-petsc:dev}"
leg2=0
leg2_native=",mpi,scotch,python,mumps,petsc,"
if [ "$CI_BACKEND" = docker ] && [ -z "${CI_NATIVE_AVAILABLE:-}" ] && [ "$petsc_image" != off ]; then
  leg2=1
fi
leg1_image="$CI_IMAGE"
leg1_target="$CI_TARGET_NAME"
: > "$(ci_file legs.tsv)"
: > "$(ci_file leg2-plan.tsv)"

meta="$(ci_file metadata.json)"
ci_step metadata --stdout "$meta" -- cargo metadata --no-deps --format-version 1 --locked
if [ "$(ci_step_status metadata)" != pass ]; then
  ci_tier_end
  exit $?
fi
python3 "$here/tools/feature_matrix.py" plan "$meta" > "$(ci_file plan.tsv)" || {
  ci_skip plan "feature_matrix.py failed"
  ci_tier_end
  exit $?
}
python3 "$here/tools/feature_matrix.py" table "$meta" > "$(ci_file features.md)" 2> /dev/null || true

while IFS="$(printf '\t')" read -r label needs args; do
  [ -n "$label" ] || continue
  missing=""
  [ "$needs" != - ] || needs=""
  for n in $(printf '%s' "$needs" | tr ',' ' '); do
    case "$native" in *",$n,"*) ;; *) missing="$missing $n" ;; esac
  done
  if [ -n "$missing" ]; then
    # petsc-only shortfall of the first leg: the second leg runs it
    # (every other prerequisite must be in the second leg's native set).
    if [ "$leg2" = 1 ]; then
      only2=1
      for n in $missing; do
        case "$leg2_native" in *",$n,"*) ;; *) only2=0 ;; esac
        case ",${CI_NATIVE_NA:-}," in *",$n,"*) only2=0 ;; esac
      done
      if [ "$only2" = 1 ]; then
        printf '%s\t%s\t%s\n' "$label" "$needs" "$args" >> "$(ci_file leg2-plan.tsv)"
        continue
      fi
    fi
    unlisted=""
    for n in $missing; do
      case ",${CI_NATIVE_NA:-}," in *",$n,"*) ;; *) unlisted="$unlisted $n" ;; esac
    done
    if [ -z "$unlisted" ]; then
      ci_na "$label" "native prerequisite(s) declared not applicable to this run (CI_NATIVE_NA):$missing"
    else
      ci_skip "$label" "native prerequisite(s) not available to the backend:$unlisted"
    fi
    continue
  fi
  printf '%s\t1\t%s\n' "$label" "$leg1_image" >> "$(ci_file legs.tsv)"
  # $args is a plain list of cargo words without quoting (from the plan).
  # shellcheck disable=SC2086
  ci_step "$label" -- env RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings" cargo $args
done < "$(ci_file plan.tsv)"

# Second leg: the checks that need PETSc, in the PETSc image.
if [ -s "$(ci_file leg2-plan.tsv)" ]; then
  CI_IMAGE="$petsc_image"
  CI_TARGET_NAME="$leg1_target-petsc"
  export CI_IMAGE CI_TARGET_NAME
  echo "leg2: native_available=$leg2_native image=$CI_IMAGE backend=$CI_BACKEND" >> "$(ci_file native.txt)"
  leg2_ready=1
  ci_backend_ready || leg2_ready=0
  while IFS="$(printf '\t')" read -r label needs args; do
    [ -n "$label" ] || continue
    if [ "$leg2_ready" = 0 ]; then
      ci_skip "$label" "needs the PETSc leg, image $CI_IMAGE absent (ci/github/images.sh <cache> mpi mumps petsc builds it from docker/petsc)"
      continue
    fi
    printf '%s\t2\t%s\n' "$label" "$CI_IMAGE" >> "$(ci_file legs.tsv)"
    # shellcheck disable=SC2086
    ci_step "$label" -- env RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings" cargo $args
  done < "$(ci_file leg2-plan.tsv)"
fi

ci_tier_end
