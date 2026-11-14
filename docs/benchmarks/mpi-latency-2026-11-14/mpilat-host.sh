#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
set -uo pipefail
cd "$HOME/akantu-rs"
cp "$HOME/mpilat.sh" "$HOME/akantu-rs/.mpilat.sh"
chmod +x "$HOME/akantu-rs/.mpilat.sh"
scripts/mpi/in-docker.sh bash /src/.mpilat.sh
rc=$?
mkdir -p "$HOME/mpilat-out"
docker run --rm -v akantu-rs-mpi-target:/target -v "$HOME/mpilat-out:/out" akantu-rs-mpi:dev bash -c 'cp -r /target/mpilat/. /out/ && chmod -R a+rwX /out'
echo "exit $rc"
exit $rc
