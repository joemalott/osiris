#!/usr/bin/env python3
"""Verify every (pack, group, offset, frames) image reference in buildings.toml,
figures.toml and menus.toml against the real PharaohData .sg3 archives.

Each .sg3 file starts with a 20 x u32 header (header[4]+1 = image record count),
followed by 300 x u16 group start indexes. Group g is valid if g == 0 or
start[g] != 0. For an anim entry referencing group g at index offset with N
frames, every record start[g]+offset .. start[g]+offset+N-1 must be < record
count (and, if g != 0, start[g] must be nonzero).

Usage: python3 verify.py [--data-dir PATH]
Exit code is 0 iff no failures were found (informational rows are always
printed to stdout as a summary).
"""
import argparse
import os
import struct
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_PHARAOH_DATA = os.path.normpath(os.path.join(HERE, "..", "..", "..", "PharaohData", "Data"))

HEADER_FMT = "<20I"
HEADER_SIZE = struct.calcsize(HEADER_FMT)
GROUP_COUNT = 300
GROUPS_FMT = f"<{GROUP_COUNT}H"
GROUPS_SIZE = struct.calcsize(GROUPS_FMT)


class SgArchive:
    def __init__(self, path):
        with open(path, "rb") as f:
            data = f.read()
        header = struct.unpack(HEADER_FMT, data[:HEADER_SIZE])
        self.record_count = header[4] + 1
        self.group_starts = struct.unpack(GROUPS_FMT, data[HEADER_SIZE:HEADER_SIZE + GROUPS_SIZE])

    def group_valid(self, group):
        if group < 0 or group >= GROUP_COUNT:
            return False
        return group == 0 or self.group_starts[group] != 0

    def check(self, group, offset, frames):
        """Return None if ok, else an error string."""
        if not self.group_valid(group):
            return f"group {group} has no start index (unused/invalid group)"
        start = self.group_starts[group]
        last = start + max(offset, 0) + max(frames, 1) - 1
        if last >= self.record_count:
            return (f"group {group} offset {offset} frames {frames} -> record "
                    f"{last} exceeds record count {self.record_count}")
        return None


def load_archives(data_dir, pack_names):
    archives = {}
    missing_files = []
    for pack in sorted(pack_names):
        path = os.path.join(data_dir, pack + ".sg3")
        if not os.path.isfile(path):
            missing_files.append(pack)
            continue
        try:
            archives[pack] = SgArchive(path)
        except Exception as e:
            missing_files.append(f"{pack} (error: {e})")
    return archives, missing_files


def collect_image_refs(doc, path_prefix=""):
    """Recursively walk a parsed TOML doc, yielding (context, pack, group, offset, frames)
    for every dict that looks like an image ref (has 'pack' and 'group' keys)."""
    refs = []

    def walk(node, ctx):
        if isinstance(node, dict):
            if "pack" in node and "group" in node:
                refs.append((ctx, node["pack"], node["group"], node.get("offset", 0), node.get("frames", 1)))
            for k, v in node.items():
                walk(v, f"{ctx}.{k}" if ctx else str(k))
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, f"{ctx}[{i}]")

    walk(doc, path_prefix)
    return refs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data-dir", default=DEFAULT_PHARAOH_DATA, help="Path to PharaohData/Data")
    args = ap.parse_args()

    toml_files = ["buildings.toml", "figures.toml", "menus.toml"]
    all_refs = []
    for fn in toml_files:
        path = os.path.join(HERE, fn)
        if not os.path.isfile(path):
            print(f"WARNING: {fn} not found, skipping", file=sys.stderr)
            continue
        with open(path, "rb") as f:
            doc = tomllib.load(f)
        refs = collect_image_refs(doc, fn)
        all_refs.extend(refs)

    pack_names = {r[1] for r in all_refs}
    archives, missing_files = load_archives(args.data_dir, pack_names)

    ok = 0
    failures = []
    for ctx, pack, group, offset, frames in all_refs:
        arc = archives.get(pack)
        if arc is None:
            failures.append((ctx, pack, group, offset, frames, f"pack file '{pack}.sg3' not found/loadable"))
            continue
        err = arc.check(group, offset, frames)
        if err:
            failures.append((ctx, pack, group, offset, frames, err))
        else:
            ok += 1

    print(f"Checked {len(all_refs)} image references across {len(toml_files)} files.")
    print(f"  OK: {ok}")
    print(f"  Failed: {len(failures)}")
    print(f"  Packs referenced: {len(pack_names)} ; loaded: {len(archives)} ; missing files: {len(missing_files)}")
    if missing_files:
        print("  Missing/unloadable pack files:")
        for m in sorted(set(missing_files)):
            print(f"    - {m}")
    if failures:
        print("\nFailures:")
        for ctx, pack, group, offset, frames, err in failures:
            print(f"  {ctx}: pack={pack} group={group} offset={offset} frames={frames} :: {err}")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
