#!/usr/bin/env python3
"""Download Tenhou Houou (phoenix room) game logs.

    python3 tools/download_logs.py --year 2026 --month 9 --out data/logs/2026-09
    python3 tools/download_logs.py --year 2026 --limit 1000 --out data/logs/sample

Tenhou publishes one game list per day, scc<YYYYMMDD>.html.gz, under
https://tenhou.net/sc/raw/dat/<YEAR>/ (the last week or so sits directly
under dat/ as hourly lists, scc<YYYYMMDDHH>.html.gz, until it is archived). Only the current year is kept there; the
lists for earlier years are no longer served. The script reads the Houou
hanchan games with red fives and open tanyao ("四鳳南喰赤") from those lists and
downloads each log as a gzipped mjlog, <id>.mjlog.gz, skipping files it
already has, so an interrupted run can simply be restarted.

Be polite to Tenhou's servers: the default delay between downloads is one
second. Standard library only.
"""

import argparse
import calendar
import gzip
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

DAY_URL = "https://tenhou.net/sc/raw/dat/{year}/scc{date}.html.gz"
HOUR_URL = "https://tenhou.net/sc/raw/dat/scc{date}{hour:02}.html.gz"
LOG_URL = "https://tenhou.net/0/log/?{id}"
# Houou, four players, hanchan, kuitan, red fives.
ROOM = "四鳳南喰赤"
LOG_ID = re.compile(r"log=(\d{10}gm-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{8})")
HEADERS = {"User-Agent": "usagi-engine-log-downloader"}


def fetch(url: str, timeout: float = 60) -> bytes:
    req = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def day_list(year: int, month: int, day: int) -> str | None:
    """One day's game list, or None if Tenhou doesn't have it."""
    date = f"{year:04}{month:02}{day:02}"
    text = fetch_list(DAY_URL.format(year=year, date=date))
    if text is not None:
        return text
    hours = [fetch_list(HOUR_URL.format(date=date, hour=h)) for h in range(24)]
    if all(h is None for h in hours):
        return None
    return "".join(h for h in hours if h is not None)


def fetch_list(url: str) -> str | None:
    try:
        return gzip.decompress(fetch(url)).decode("utf-8", errors="replace")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise


def houou_ids(year: int, month: int | None) -> list[str]:
    """Game ids from the daily game lists, in date order."""
    ids = []
    for m in [month] if month is not None else range(1, 13):
        missing = 0
        for d in range(1, calendar.monthrange(year, m)[1] + 1):
            text = day_list(year, m, d)
            if text is None:
                missing += 1
                continue
            for line in text.splitlines():
                if ROOM in line:
                    found = LOG_ID.search(line)
                    if found:
                        ids.append(found.group(1))
        print(f"{year}-{m:02}: {len(ids)} games so far, {missing} days missing", file=sys.stderr)
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
