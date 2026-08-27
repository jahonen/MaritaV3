#!/usr/bin/env python3
"""Generate an initial-condition JSON snapshot from local SPICE kernels.

Usage:
    python scripts/generate_ephemeris.py \
        --kernels-dir ./kernels \
        --epoch 2026-01-01T00:00:00Z \
        --out data/ephemeris.json

Requires:
    pip install spiceypy

Recommended kernels (download from https://naif.jpl.nasa.gov/naif/data.html):
    - de440.bsp               (planetary ephemeris)
    - pck00011.tpc            (physical constants and radii)
    - naif0012.tls            (leap seconds)
    - some satellite ephemeris for major moons (optional)
"""

import argparse
import json
import os
import sys
from pathlib import Path

from spiceypy import spiceypy as spice

# NAIF IDs for major solar-system bodies typically covered by de440.bsp and
# common satellite ephemeris kernels. Use --id/--name to extend the list.
DEFAULT_TARGETS = [
    (10, "Sun"),
    (199, "Mercury"),
    (299, "Venus"),
    (399, "Earth"),
    (301, "Moon"),
    (499, "Mars"),
    (401, "Phobos"),
    (402, "Deimos"),
    (599, "Jupiter"),
    (501, "Io"),
    (502, "Europa"),
    (503, "Ganymede"),
    (504, "Callisto"),
    (699, "Saturn"),
    (601, "Mimas"),
    (602, "Enceladus"),
    (603, "Tethys"),
    (604, "Dione"),
    (605, "Rhea"),
    (606, "Titan"),
    (607, "Hyperion"),
    (608, "Iapetus"),
    (609, "Phoebe"),
    (799, "Uranus"),
    (715, "Miranda"),
    (701, "Ariel"),
    (702, "Umbriel"),
    (703, "Titania"),
    (704, "Oberon"),
    (899, "Neptune"),
    (801, "Triton"),
    (802, "Nereid"),
    (803, "Naiad"),
    (804, "Thalassa"),
    (805, "Despina"),
    (806, "Galatea"),
    (807, "Larissa"),
    (808, "Proteus"),
    (999, "Pluto"),
    (901, "Charon"),
    (2000001, "Ceres"),
    (2000002, "Pallas"),
    (2000004, "Vesta"),
    (2000010, "Hygiea"),
    (2000016, "Psyche"),
    (2000043, "Davida"),
    (2000044, "Interamnia"),
    (2000052, "Europa"),
    (2000066, "Sylvia"),
]


def load_kernels(kernels_dir: Path):
    kernels = []
    for ext in ("*.bsp", "*.tpc", "*.tls", "*.tf", "*.bc"):
        kernels.extend(kernels_dir.glob(ext))
    if not kernels:
        raise FileNotFoundError(f"no SPICE kernels found in {kernels_dir}")
    for k in kernels:
        spice.furnsh(str(k))
        print(f"loaded kernel: {k}", file=sys.stderr)


def get_state(target_id: int, epoch_et: float):
    # State relative to solar-system barycenter in the ecliptic J2000 frame.
    state, _ = spice.spkezr(str(target_id), epoch_et, "ECLIPJ2000", "NONE", "0")
    x, y, z, vx, vy, vz = state
    return x * 1000.0, y * 1000.0, z * 1000.0, vx * 1000.0, vy * 1000.0, vz * 1000.0


def get_radius(target_id: int):
    try:
        # Try to read radii from PCK.
        dim, radii = spice.bodvcd(target_id, "RADII", 3)
        # Use mean radius.
        return float(sum(radii[:dim]) / dim)
    except Exception:
        pass

    # Fallback approximate radii in km for major bodies.
    fallbacks = {
        10: 696340.0,
        199: 2439.7,
        299: 6051.8,
        399: 6371.0,
        301: 1737.4,
        499: 3389.5,
        401: 11.08,
        402: 6.2,
        599: 69911.0,
        501: 1821.6,
        502: 1560.8,
        503: 2631.2,
        504: 2410.3,
        699: 58232.0,
        601: 198.2,
        602: 252.1,
        603: 531.0,
        604: 561.7,
        605: 764.3,
        606: 2574.7,
        607: 135.0,
        608: 734.5,
        799: 25362.0,
        701: 578.9,
        702: 584.7,
        703: 788.9,
        704: 761.4,
        899: 24622.0,
        801: 1353.4,
        999: 1188.3,
        901: 606.0,
        2000001: 469.7,
        2000002: 256.0,
        2000004: 262.7,
        2000010: 225.0,
    }
    return fallbacks.get(target_id, 100.0)


def main():
    parser = argparse.ArgumentParser(description="Generate ephemeris JSON from SPICE kernels")
    parser.add_argument("--kernels-dir", type=Path, required=True)
    parser.add_argument("--epoch", type=str, default="2026-01-01T00:00:00Z")
    parser.add_argument("--out", type=Path, default=Path("data/ephemeris.json"))
    parser.add_argument("--id", type=int, action="append", default=None)
    parser.add_argument("--name", type=str, action="append", default=None)
    args = parser.parse_args()

    load_kernels(args.kernels_dir)

    if args.id and args.name:
        if len(args.id) != len(args.name):
            raise ValueError("--id and --name lists must have the same length")
        targets = list(zip(args.id, args.name))
    else:
        targets = DEFAULT_TARGETS

    epoch_et = spice.str2et(args.epoch)
    print(f"epoch ET: {epoch_et:.3f} ({args.epoch})", file=sys.stderr)

    bodies = []
    for target_id, name in targets:
        try:
            x, y, z, vx, vy, vz = get_state(target_id, epoch_et)
            radius_km = get_radius(target_id)
            bodies.append(
                {
                    "id": target_id,
                    "name": name,
                    "mass": 0.0,  # mass not loaded from SPICE; set separately if needed
                    "position": {"x": x, "y": y, "z": z},
                    "velocity": {"x": vx, "y": vy, "z": vz},
                    "radius": radius_km * 1000.0,
                }
            )
        except Exception as e:
            print(f"skipped {name} ({target_id}): {e}", file=sys.stderr)

    spice.kclear()

    out = {
        "epoch": args.epoch,
        "frame": "ECLIPJ2000",
        "observer": "SOLAR_SYSTEM_BARYCENTER",
        "bodies": bodies,
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(out, f, indent=2)

    print(f"wrote {len(bodies)} bodies to {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
