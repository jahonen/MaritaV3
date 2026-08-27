#!/usr/bin/env python3
"""Generate an initial-condition JSON snapshot from JPL Horizons.

This is an alternative to `generate_ephemeris.py` for users who do not want to
download multi-gigabyte SPICE satellite ephemeris kernels. It queries the public
JPL Horizons API for the same solar-system bodies and writes a snapshot in the
same format.

Usage:
    python scripts/generate_ephemeris_horizons.py \
        --epoch 2026-01-01T00:00:00Z \
        --out data/ephemeris.json
"""

import argparse
import datetime
import json
import time
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Optional, Tuple

# (Horizons COMMAND, name, approximate radius in km for bodies not in PCK)
# (Horizons COMMAND, name, approximate radius in km)
# Asteroids are suffixed with ";" to tell Horizons they are small bodies, avoiding
# collisions with planet/major-body numeric IDs.
DEFAULT_TARGETS = [
    ("10", "Sun", 696340.0),
    ("199", "Mercury", 2439.7),
    ("299", "Venus", 6051.8),
    ("399", "Earth", 6371.0),
    ("301", "Moon", 1737.4),
    ("499", "Mars", 3389.5),
    ("401", "Phobos", 11.08),
    ("402", "Deimos", 6.2),
    ("599", "Jupiter", 69911.0),
    ("501", "Io", 1821.6),
    ("502", "Europa", 1560.8),
    ("503", "Ganymede", 2631.2),
    ("504", "Callisto", 2410.3),
    ("699", "Saturn", 58232.0),
    ("601", "Mimas", 198.2),
    ("602", "Enceladus", 252.1),
    ("603", "Tethys", 531.0),
    ("604", "Dione", 561.7),
    ("605", "Rhea", 764.3),
    ("606", "Titan", 2574.7),
    ("607", "Hyperion", 135.0),
    ("608", "Iapetus", 734.5),
    ("609", "Phoebe", 106.5),
    ("799", "Uranus", 25362.0),
    ("715", "Miranda", 235.8),
    ("701", "Ariel", 578.9),
    ("702", "Umbriel", 584.7),
    ("703", "Titania", 788.9),
    ("704", "Oberon", 761.4),
    ("899", "Neptune", 24622.0),
    ("801", "Triton", 1353.4),
    ("802", "Nereid", 170.0),
    ("803", "Naiad", 33.0),
    ("804", "Thalassa", 40.0),
    ("805", "Despina", 75.0),
    ("806", "Galatea", 79.0),
    ("807", "Larissa", 97.0),
    ("808", "Proteus", 210.0),
    ("999", "Pluto", 1188.3),
    ("901", "Charon", 606.0),
    ("1;", "Ceres", 469.7),
    ("2;", "Pallas", 256.0),
    ("4;", "Vesta", 262.7),
    ("10;", "Hygiea", 225.0),
    ("16;", "Psyche", 113.0),
    ("511;", "Davida", 162.0),
    ("704;", "Interamnia", 158.0),
    ("52;", "Europa (asteroid)", 151.5),
    ("87;", "Sylvia", 136.0),
]

HORIZONS_URL = "https://ssd.jpl.nasa.gov/api/horizons.api"


def fetch_state(command: str, epoch: str) -> Optional[Tuple[float, float, float, float, float, float]]:
    # Horizons requires start < stop, so request a one-day span and use the
    # first vector, which corresponds to the requested epoch.
    start = epoch
    try:
        dt = datetime.datetime.fromisoformat(epoch.replace("Z", "+00:00"))
        stop_dt = dt + datetime.timedelta(days=1)
        stop = stop_dt.strftime("%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        stop = epoch

    params = {
        "format": "text",
        "COMMAND": f"'{command}'",
        "CENTER": "'500@0'",
        "MAKE_EPHEM": "'YES'",
        "TABLE_TYPE": "'VECTORS'",
        "START_TIME": f"'{start}'",
        "STOP_TIME": f"'{stop}'",
        "STEP_SIZE": "'1 d'",
        "OUT_UNITS": "'KM-S'",
        "REF_PLANE": "'ECLIPTIC'",
        "VEC_LABELS": "'NO'",
        "VEC_CORR": "'NONE'",
    }
    query = urllib.parse.urlencode(params)
    url = f"{HORIZONS_URL}?{query}"

    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            text = response.read().decode("utf-8")
    except Exception as e:
        print(f"network error for {command}: {e}", file=__import__("sys").stderr)
        return None

    start = text.find("$$SOE")
    end = text.find("$$EOE")
    if start == -1 or end == -1 or end <= start:
        print(f"no ephemeris data for {command}", file=__import__("sys").stderr)
        return None

    block = text[start + len("$$SOE") : end].strip()
    lines = [ln for ln in block.splitlines() if ln.strip()]
    if len(lines) < 4:
        return None

    # Horizons vector table format for each time step:
    #   2461041.500000000 = A.D. 2026-Jan-01 00:00:00.0000 TDB
    #     X  Y  Z
    #     VX VY VZ
    #     LT RG RR
    def parse_line_of_three(line: str) -> Optional[Tuple[float, float, float]]:
        try:
            tokens = line.split()
            if len(tokens) < 3:
                return None
            return float(tokens[0]), float(tokens[1]), float(tokens[2])
        except ValueError:
            return None

    pos = parse_line_of_three(lines[1])
    vel = parse_line_of_three(lines[2])
    if pos is None or vel is None:
        print(f"failed to parse vector lines for {command}", file=__import__("sys").stderr)
        return None

    x, y, z = pos
    vx, vy, vz = vel
    return x * 1000.0, y * 1000.0, z * 1000.0, vx * 1000.0, vy * 1000.0, vz * 1000.0


def main():
    parser = argparse.ArgumentParser(description="Generate ephemeris JSON from JPL Horizons")
    parser.add_argument("--epoch", type=str, default="2026-01-01T00:00:00Z")
    parser.add_argument("--out", type=Path, default=Path("data/ephemeris.json"))
    parser.add_argument("--delay", type=float, default=0.2, help="seconds between API calls")
    args = parser.parse_args()

    bodies = []
    for command, name, fallback_radius in DEFAULT_TARGETS:
        print(f"fetching {name} ({command}) ...", file=__import__("sys").stderr)
        state = fetch_state(command, args.epoch)
        if state is None:
            print(f"skipped {name}", file=__import__("sys").stderr)
            continue
        x, y, z, vx, vy, vz = state
        clean_command = command.replace("'", "").rstrip(";")
        bodies.append(
            {
                "id": int(clean_command),
                "name": name,
                "mass": 0.0,
                "position": {"x": x, "y": y, "z": z},
                "velocity": {"x": vx, "y": vy, "z": vz},
                "radius": fallback_radius * 1000.0,
            }
        )
        time.sleep(args.delay)

    out = {
        "epoch": args.epoch,
        "frame": "ECLIPJ2000",
        "observer": "SOLAR_SYSTEM_BARYCENTER",
        "source": "JPL Horizons",
        "bodies": bodies,
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(out, f, indent=2)

    print(f"wrote {len(bodies)} bodies to {args.out}", file=__import__("sys").stderr)


if __name__ == "__main__":
    main()
