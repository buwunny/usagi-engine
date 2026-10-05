#!/usr/bin/env python3
"""Cut one hand out of a Tenhou log as an anonymised test excerpt.

    python3 tools/excerpt_log.py LOG HAND OUT

LOG is an mjlog (plain or gzipped), HAND is the hand number that
usagi-replay reports (from 0), OUT is the .mjlog to write. Player
names and ranks are replaced, and the replayer checks the excerpt up to
where it stops.
"""
import gzip, re, sys
src, hand, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
s = (gzip.open(src) if src.endswith(".gz") else open(src, "rb")).read().decode()
tags = re.findall(r"<[^>]+>", s)
head = [t for t in tags if t.startswith(("<GO", "<TAIKYOKU"))]
# Hands start at INIT; keep the requested one up to the next INIT (or the end).
starts = [i for i, t in enumerate(tags) if t.startswith("<INIT")]
end = starts[hand + 1] if hand + 1 < len(starts) else len(tags) - 1
body = [t for t in tags[starts[hand]:end] if not t.startswith(("<BYE", "<UN"))]
un = '<UN n0="A" n1="B" n2="C" n3="D" dan="0,0,0,0" rate="1500,1500,1500,1500" sx="M,M,M,M"/>'
xml = '<mjloggm ver="2.3">' + head[0] + un + "".join(head[1:]) + "".join(body) + "</mjloggm>"
open(out, "w").write(xml)
