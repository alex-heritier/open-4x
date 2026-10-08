#!/usr/bin/env python3
"""Stage the selected scenario cache for the browser build (a Trunk `post_build` hook).

The cache can exceed a gigabyte, so Trunk's `copy-dir` made every build a
long copy and a second copy on disk. This does two cheaper things:

* Writes `<dist>/web-bundle.json`: the small text files the game reads
  while it starts and plays (unit manifests, clip lists, the request), keyed
  by URL. It goes beside the page; the converter owns the cache. In a browser every `read_text` is a blocking request, so a
  unit type's first appearance would freeze the page for a round trip; with
  the bundle they are one request at startup.

* Makes `<dist>/assets` point at `.cache/civ3/` (or `CIV3_CACHE`): a symlink
  by default (instant; `trunk serve` and `python3 -m http.server` follow it), or with
  `--copy` a tree of hard links (real files, no extra disk space, for hosts
  and upload tools that do not follow symlinks). `index.json`, the
  converter's bookkeeping, is left out of the hard-link tree.

Usage: web_assets.py [--copy] [--dest DIR]   (DIR defaults to $TRUNK_STAGING_DIR)
"""
import argparse
import json
import os
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.abspath(os.environ.get("CIV3_CACHE", os.path.join(ROOT, ".cache", "civ3")))
URL = "assets"
BUNDLE = "web-bundle.json"

# Bookkeeping the game never reads.
SKIP = {"index.json", BUNDLE}
# Read when first needed rather than at startup; keeping them out of the
# bundle keeps the one blocking startup request small.
LAZY = {"text/diplomacy.txt"}
BUNDLED = (".json", ".txt")


def walk():
    """(relative path, absolute path) of every file of the cache."""
    for here, _, names in os.walk(CACHE):
        rel_dir = os.path.relpath(here, CACHE)
        for name in names:
            rel = name if rel_dir == "." else f"{rel_dir}/{name}".replace(os.sep, "/")
            yield rel, os.path.join(here, name)


def write_bundle(dist):
    bundle = {}
    for rel, path in walk():
        if rel in SKIP or rel in LAZY or not rel.endswith(BUNDLED):
            continue
        try:
            with open(path, encoding="utf-8") as f:
                bundle[f"{URL}/{rel}"] = f.read()
        except UnicodeDecodeError:
            pass
    out = os.path.join(dist, BUNDLE)
    os.makedirs(dist, exist_ok=True)
    with open(out, "w", encoding="utf-8") as f:
        json.dump(bundle, f, separators=(",", ":"))
    return len(bundle), os.path.getsize(out)


def hard_link_tree(dest):
    count = 0
    for rel, path in walk():
        if rel == "index.json":
            continue
        out = os.path.join(dest, *rel.split("/"))
        os.makedirs(os.path.dirname(out), exist_ok=True)
        try:
            os.link(path, out)
        except OSError:  # another file system
            shutil.copy2(path, out)
        count += 1
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--dest", help="the dist or staging directory (default $TRUNK_STAGING_DIR)")
    parser.add_argument("--copy", action="store_true", help="hard-link real files instead of a symlink")
    args = parser.parse_args()
    dist = args.dest or os.environ.get("TRUNK_STAGING_DIR")
    if not dist:
        sys.exit("web_assets: no --dest and no TRUNK_STAGING_DIR")
    if not os.path.isdir(CACHE):
        print(f"web_assets: {CACHE} is missing; the page will have no art (run tools/prep_assets.py)", file=sys.stderr)
        return
    bundled, size = write_bundle(dist)
    dest = os.path.join(dist, "assets")
    if os.path.islink(dest):
        os.unlink(dest)
    else:
        shutil.rmtree(dest, ignore_errors=True)
    if args.copy:
        files = hard_link_tree(dest)
        print(f"web_assets: {files} files hard-linked into {dest}; bundle {bundled} files, {size / 1e6:.2f} MB")
    else:
        os.symlink(CACHE, dest)
        print(f"web_assets: {dest} -> {CACHE}; bundle {bundled} files, {size / 1e6:.2f} MB")


if __name__ == "__main__":
    main()
