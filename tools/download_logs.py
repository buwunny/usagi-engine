#!/usr/bin/env python3
"""Download Tenhou Houou (phoenix room) game logs.

    python3 tools/download_logs.py --year 2023 --month 1 --out data/logs/2023-01
    python3 tools/download_logs.py --year 2023 --limit 1000 --out data/logs/sample

Tenhou publishes a yearly archive of game lists (scraw<YEAR>.zip, one
scc<YYYYMMDD>.html.gz file per day). The script reads the Houou hanchan
games with red fives and open tanyao ("四鳳南喰赤") from those lists and
downloads each log as a gzipped mjlog, <id>.mjlog.gz, skipping files it
already has, so an interrupted run can simply be restarted.

Be polite to Tenhou's servers: the default delay between downloads is one
second. Standard library only.
"""

import argparse
import gzip
import io
import re
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

ARCHIVE_URL = "https://tenhou.net/sc/raw/scraw{year}.zip"
LOG_URL = "https://tenhou.net/0/log/?{id}"
# Houou, four players, hanchan, kuitan, red fives.
ROOM = "四鳳南喰赤"
LOG_ID = re.compile(r"log=(\d{10}gm-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{8})")
HEADERS = {"User-Agent": "mochitsuki-log-downloader"}


def fetch(url: str, timeout: float = 60) -> bytes:
    req = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def houou_ids(year: int, month: int | None) -> list[str]:
    """Game ids from the yearly archive, in date order."""
    print(f"downloading the {year} game list...", file=sys.stderr)
    archive = zipfile.ZipFile(io.BytesIO(fetch(ARCHIVE_URL.format(year=year), timeout=600)))
    ids = []
    for name in sorted(archive.namelist()):
        day = re.search(r"scc(\d{8})\.html\.gz$", name)
        if not day:
            continue
        if month is not None and int(day.group(1)[4:6]) != month:
            continue
        text = gzip.decompress(archive.read(name)).decode("utf-8", errors="replace")
        for line in text.splitlines():
            if ROOM in line:
                m = LOG_ID.search(line)
                if m:
                    ids.append(m.group(1))
    return ids


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--year", type=int, required=True)
    ap.add_argument("--month", type=int, help="only this month (1-12)")
    ap.add_argument("--limit", type=int, help="stop after this many games")
    ap.add_argument("--out", type=Path, required=True, help="output directory")
    ap.add_argument("--delay", type=float, default=1.0, help="seconds between downloads")
    args = ap.parse_args()

    ids = houou_ids(args.year, args.month)
    if args.limit is not None:
        ids = ids[: args.limit]
    print(f"{len(ids)} Houou games", file=sys.stderr)
    args.out.mkdir(parents=True, exist_ok=True)

    done = failed = 0
    for i, game_id in enumerate(ids, 1):
        path = args.out / f"{game_id}.mjlog.gz"
        if path.exists():
            continue
        try:
            xml = fetch(LOG_URL.format(id=game_id))
        except Exception as e:  # keep going; a rerun retries what failed
            print(f"{game_id}: {e}", file=sys.stderr)
            failed += 1
            continue
        tmp = path.with_suffix(".tmp")
        tmp.write_bytes(gzip.compress(xml))
        tmp.rename(path)
        done += 1
        if i % 100 == 0:
            print(f"{i}/{len(ids)}", file=sys.stderr)
        time.sleep(args.delay)
    print(f"downloaded {done}, failed {failed}, in {args.out}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
