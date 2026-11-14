#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): builds the port's Python
# wheel (maturin, release, in the project image akantu-rs-mpi:dev) and unpacks
# it to <out>/port (the directory the Python examples put on PYTHONPATH).
#   scripts/paraview/build-wheel.sh <out-dir>
# Host: an x86_64 Linux host (docs/TESTING.md).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
out="${1:?usage: build-wheel.sh <out-dir>}"
mkdir -p "${out}"; out="$(cd "${out}" && pwd)"
image="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
# root in the container (the image's default user cannot run apt), with its own
# directory in the target volume so that the akantu user's builds are untouched;
# the image has no pip: installed in this throw-away container (docs/TESTING.md,
# "proposal for a python tier")
docker run --rm --init --user root --memory 6g --memory-swap 6g --cpus 4 \
  -e HOME=/tmp -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  -v "${repo}:/src:ro" -v akantu-rs-mpi-target:/target -v akantu-rs-mpi-cargo:/usr/local/cargo/registry \
  -v "${out}:/out" -e CARGO_TARGET_DIR=/target/python-wheel -e CARGO_INCREMENTAL=0 \
  -w /src/python "${image}" bash -c \
  "apt-get -qq update && apt-get -qq -y --no-install-recommends install python3-pip patchelf >/dev/null && pip install -q --break-system-packages maturin==1.16.0 && maturin build --release --locked -o /out/wheels && chown -R \${HOST_UID}:\${HOST_GID} /out"
rm -rf "${out}/port"; mkdir -p "${out}/port"
python3 -c "import sys,zipfile,glob; [zipfile.ZipFile(w).extractall(sys.argv[1]) for w in glob.glob(sys.argv[2]+'/wheels/*.whl')]" "${out}/port" "${out}"
ls "${out}/port" "${out}/wheels"
