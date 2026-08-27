#!/usr/bin/env bash
# Download the recommended local SPICE kernels for MaritaV3.
#
# The "lite" set (planets + Moon) is a few hundred MB and gives ~5 bodies.
# The "full" set adds satellite ephemerides and can be several GB; it is
# required if you want all ~50 bodies in the default target list.
#
# Usage:
#   scripts/download_kernels.sh [lite|full]

set -euo pipefail

MODE="${1:-lite}"
KERNELS_DIR="${KERNELS_DIR:-./kernels}"
mkdir -p "$KERNELS_DIR"

cd "$KERNELS_DIR"

echo "Downloading $MODE SPICE kernels to $KERNELS_DIR ..."

# Always fetch the core kernels.
curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/de440.bsp
curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/pck00011.tpc
curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/lsk/naif0012.tls

if [ "$MODE" = "full" ]; then
  # Satellite ephemerides. These are large (each can be hundreds of MB to >1 GB).
  curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/mar097.bsp
  curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/jup365.bsp
  curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/sat441.bsp
  curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/ura111.bsp
  curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/nep081.bsp
  curl -L -O https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/plu055.bsp
fi

echo "Done. Run the snapshot generator with:"
echo "  source .venv/bin/activate"
echo "  python scripts/generate_ephemeris.py --kernels-dir $KERNELS_DIR --out data/ephemeris.json"
