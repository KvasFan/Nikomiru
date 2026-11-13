#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""PLY-023 fixture: fixed local metadata, HTTP subtitle downloads and a slow open."""
import json
from pathlib import Path
import sys
import time
import urllib.request

script = Path(__file__)
args = sys.argv[1:]
script.with_suffix('.args.json').write_text(json.dumps(args), encoding='utf-8')
if args[-1].endswith('/slow'):
    time.sleep(5)
metadata = json.loads(script.with_suffix('.json').read_text(encoding='utf-8'))
if '--output' in args:
    pattern = args[args.index('--output') + 1]
    for language, subtitles in metadata.get('subtitles', {}).items():
        subtitle = subtitles[0]
        path = Path(pattern.replace('%(language)s', language).replace('%(ext)s', subtitle['ext']))
        path.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(subtitle['url'], timeout=5) as response:
            path.write_bytes(response.read())
if '--print' in args:
    fields = ('title', 'url', 'protocol', 'requested_formats', 'http_headers', 'is_live', 'has_drm')
    metadata = {key: value for key, value in metadata.items() if key in fields}
print(json.dumps(metadata, ensure_ascii=False))
