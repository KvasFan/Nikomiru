#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

"""Small ffprobe/ffmpeg stand-ins for score source/cache/cancellation checks."""

import json
import pathlib
import sys
import time

root = pathlib.Path(sys.argv[0]).parent
kind = "probe" if pathlib.Path(sys.argv[0]).name == "ffprobe" else "extract"
with (root / "calls").open("a") as log:
    log.write(kind + "\n")
(root / (kind + ".started")).touch()
if kind == "extract":
    out = pathlib.Path(sys.argv[-1].removeprefix("file:"))
    out.write_text("partial", encoding="utf-8")
if (root / (kind + ".wait")).exists():
    time.sleep(60)
if kind == "probe":
    print((root / "metadata.json").read_text(encoding="utf-8"))
else:
    index = sys.argv[sys.argv.index("-map") + 1].split(":")[-1]
    source = root / ("stream-" + index + ".txt")
    out.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
