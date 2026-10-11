#!/usr/bin/env python3
"""Reconstruct the upstream Carbonyl patch result against its official base.

Only downloads files touched by Carbonyl; does not fetch another fork.
The result is a reference for porting to current official Chromium.
"""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import re
import shutil
import subprocess
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
for component, repository, revision in (
    ('chromium', 'chromium/chromium', '92da8189788b1b373cbd3348f73d695dfdc521b6'),
    ('skia', 'google/skia', '486deb23bc2a4d3d09c66fef52c2ad64d8b4f761'),
):
    base = ROOT / 'build/port-reference' / component / 'base'
    patched = base.parent / 'patched'
    patches = sorted((ROOT / 'chromium/patches' / component).glob('*.patch'))
    paths = set()
    for patch in patches:
        for section in re.split(r'^diff --git ', patch.read_text(), flags=re.M)[1:]:
            name = section.splitlines()[0].split()[0][2:]
            index = re.search(r'^index ([0-9a-f]+)\.\.', section, re.M)
            if index and int(index.group(1), 16) != 0:
                paths.add(name)
    def download(name):
        destination = base / name
        if destination.exists():
            return
        with urlopen(f'https://raw.githubusercontent.com/{repository}/{revision}/{name}', timeout=60) as response:
            data = response.read()
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        print(component, name, len(data), flush=True)
    with ThreadPoolExecutor(max_workers=8) as pool:
        list(pool.map(download, sorted(paths)))
    if not patched.exists():
        shutil.copytree(base, patched)
        subprocess.run(['git', 'init', '-q'], cwd=patched, check=True)
        for patch in patches:
            subprocess.run(['git', 'apply', '--whitespace=nowarn', str(patch)], cwd=patched, check=True)
    print('Ready:', patched, flush=True)
