#!/usr/bin/env python3
"""Download Tenhou Houou (phoenix room) game logs.

    python3 tools/download_logs.py --year 2026 --month 9 --out data/logs/2026-09
    python3 tools/download_logs.py --year 2026 --limit 1000 --out data/logs/sample

Tenhou publishes one game list per day, scc<YYYYMMDD>.html.gz, under
https://tenhou.net/sc/raw/dat/<YEAR>/ (the last week or so sits directly
under dat/ as hourly lists, scc<YYYYMMDDHH>.html.gz, until it is
archived). Only the current year is kept there; the lists for earlier
years are no longer served. The script reads the Houou hanchan games with
red fives and open tanyao ("四鳳南喰赤") from those lists and downloads
each log as a gzipped mjlog, <id>.mjlog.gz, skipping files it already has,
so an interrupted run can simply be restarted.

Progress is shown as a bar on stderr when it is a terminal, and as a line
every 100 games otherwise (for logs and CI).

Be polite to Tenhou's servers: the default delay between downloads is one
second. Standard library only.
"""

import argparse
import calendar
import datetime
import gzip
import re
import shutil
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


class Progress:
    """A one-line progress bar on stderr.

    On a terminal the line is redrawn in place; otherwise a plain line is
    printed every `every` steps and at the end. `note` prints a message
    above the bar without garbling it.
    """

    def __init__(self, total: int, label: str, every: int = 100):
        self.total, self.label, self.every = total, label, every
        self.n = 0
        self.extra = ""
        self.start = time.monotonic()
        self.tty = sys.stderr.isatty()
        self.width = 0

    def update(self, n: int = 1, extra: str | None = None) -> None:
        self.n += n
        if extra is not None:
            self.extra = extra
        if self.tty:
            self._draw()
        elif self.n % self.every == 0 or self.n == self.total:
            print(self._text(bar=False), file=sys.stderr, flush=True)

    def note(self, message: str) -> None:
        if self.tty:
            self._clear()
        print(message, file=sys.stderr, flush=True)
        if self.tty:
            self._draw()

    def close(self) -> None:
        if self.tty:
            self._draw()
            print(file=sys.stderr, flush=True)

    def _text(self, bar: bool) -> str:
        total = max(self.total, 1)
        elapsed = time.monotonic() - self.start
        rate = self.n / elapsed if elapsed > 0 else 0.0
        eta = (self.total - self.n) / rate if rate > 0 else None
        parts = [
            f"{self.label} {self.n}/{self.total} ({100 * self.n / total:.0f}%)",
            f"{fmt_time(elapsed)} elapsed",
            f"ETA {fmt_time(eta)}" if eta is not None else "ETA --",
        ]
        if self.extra:
            parts.append(self.extra)
        text = ", ".join(parts)
        if bar:
            cols = shutil.get_terminal_size((100, 20)).columns
            room = cols - len(text) - 3
            if room >= 10:
                size = min(room, 40)
                filled = size * self.n // total
                text = f"[{'#' * filled}{'.' * (size - filled)}] {text}"
        return text

    def _draw(self) -> None:
        text = self._text(bar=True)
        pad = max(self.width - len(text), 0)
        sys.stderr.write("\r" + text + " " * pad)
        sys.stderr.flush()
        self.width = len(text)

    def _clear(self) -> None:
        sys.stderr.write("\r" + " " * self.width + "\r")
        self.width = 0


def fmt_time(seconds: float | None) -> str:
    if seconds is None:
        return "--"
    s = int(seconds)
    h, s = divmod(s, 3600)
    m, s = divmod(s, 60)
    return f"{h}:{m:02}:{s:02}" if h else f"{m}:{s:02}"


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
    months = [month] if month is not None else list(range(1, 13))
    # Tenhou dates are Japan time; days that haven't started there have no list.
    today = datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=9))).date()
    days = [
        (m, d)
        for m in months
        for d in range(1, calendar.monthrange(year, m)[1] + 1)
        if datetime.date(year, m, d) <= today
    ]
    ids = []
    missing = 0
    bar = Progress(len(days), "game lists", every=10)
    for m, d in days:
        text = day_list(year, m, d)
        if text is None:
            missing += 1
        else:
            for line in text.splitlines():
                if ROOM in line:
                    found = LOG_ID.search(line)
                    if found:
                        ids.append(found.group(1))
        bar.update(extra=f"{len(ids)} games, {missing} days missing")
    bar.close()
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
    args.out.mkdir(parents=True, exist_ok=True)
    todo = [i for i in ids if not (args.out / f"{i}.mjlog.gz").exists()]
    print(
        f"{len(ids)} Houou games, {len(ids) - len(todo)} already in {args.out}, "
        f"{len(todo)} to download",
        file=sys.stderr,
    )

    done = failed = 0
    bar = Progress(len(todo), "logs")
    for game_id in todo:
        path = args.out / f"{game_id}.mjlog.gz"
        try:
            xml = fetch(LOG_URL.format(id=game_id))
        except Exception as e:  # keep going; a rerun retries what failed
            bar.note(f"{game_id}: {e}")
            failed += 1
        else:
            tmp = path.with_suffix(".tmp")
            tmp.write_bytes(gzip.compress(xml))
            tmp.rename(path)
            done += 1
        bar.update(extra=f"{failed} failed" if failed else "")
        time.sleep(args.delay)
    bar.close()
    print(f"downloaded {done}, failed {failed}, in {args.out}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
