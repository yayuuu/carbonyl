#!/usr/bin/env python3
"""Stage or install the locally built original Carbonyl + Chromium 154."""
import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'chromium/src/out/Carbonyl'
parser = argparse.ArgumentParser()
parser.add_argument('action', choices=['stage', 'install', 'archive'])
parser.add_argument('--backend', choices=['headless', 'chrome'], default='headless')
parser.add_argument('--require-tested', action='store_true',
                    help='Require receipts from the local integration test harness before installing')
args = parser.parse_args()
variant = '-chrome' if args.backend == 'chrome' else ''
STAGE = ROOT / f'build/carbonyl154{variant}-runtime'
DEST = Path(f'/usr/local/lib/carbonyl-0.0.3-chromium154{variant}')

if args.action == 'archive':
    destination = ROOT / 'chromium/patches/chromium154'
    destination.mkdir(exist_ok=True)
    (destination / 'build-args.gn').write_text((OUT / 'args.gn').read_text())
    chromium = subprocess.check_output(['git', '-C', str(ROOT / 'chromium/src'), 'diff', '--binary'])
    for relative in ('components/viz/service/display_embedder/software_output_device_proxy.cc',
                     'components/viz/service/display_embedder/software_output_device_proxy.h',
                     'chrome/browser/carbonyl_browser_input.cc',
                     'chrome/browser/carbonyl_dialogs.cc',
                     'chrome/browser/carbonyl_dialogs.h',
                     'chrome/browser/carbonyl_clipboard.cc',
                     'chrome/browser/carbonyl_clipboard.h',
                     'ui/base/clipboard/carbonyl_clipboard.h'):
        result = subprocess.run(['git', 'diff', '--no-index', '--', '/dev/null', relative],
            cwd=ROOT / 'chromium/src', stdout=subprocess.PIPE)
        assert result.returncode in (0, 1)
        chromium += result.stdout
    (destination / 'chromium.patch').write_bytes(chromium)
    (destination / 'skia.patch').write_bytes(subprocess.check_output([
        'git', '-C', str(ROOT / 'chromium/src/third_party/skia'), 'diff', '--binary']))
    print('Saved reproducible Chromium and Skia patches:', destination)
    raise SystemExit()

if args.action == 'stage':
    binary = OUT / ('chrome' if args.backend == 'chrome' else 'headless_shell')
    library = ROOT / 'build/x86_64-unknown-linux-gnu/release/libcarbonyl.so'
    assert binary.is_file() and library.is_file(), 'Build Carbonyl and headless_shell first'
    STAGE.mkdir(exist_ok=True)
    shutil.copy2(binary, STAGE / 'carbonyl')
    shutil.copy2(library, STAGE / 'libcarbonyl.so')
    if args.backend == 'chrome':
        shutil.copy2(ROOT / 'scripts/carbonyl-file-portal.py', STAGE / 'carbonyl-file-portal.py')
    for name in ('icudtl.dat', 'v8_context_snapshot.bin', 'snapshot_blob.bin',
                 'libEGL.so', 'libGLESv2.so', 'libvk_swiftshader.so',
                 'libvulkan.so.1', 'vk_swiftshader_icd.json', 'headless_lib.pak',
                 'resources.pak', 'chrome_100_percent.pak', 'chrome_200_percent.pak',
                 'chrome_crashpad_handler', 'chrome_sandbox'):
        source = OUT / name
        if source.exists():
            shutil.copy2(source, STAGE / name)
    if (OUT / 'locales').is_dir():
        shutil.copytree(OUT / 'locales', STAGE / 'locales', dirs_exist_ok=True)
    shutil.copy2(ROOT / 'chromium/src/LICENSE', STAGE / 'Chromium-LICENSE')
    manifest = {'carbonyl': '0.0.3', 'chromium': '154.0.8037.97',
        'upstream': 'https://github.com/fathyb/carbonyl', 'backend': args.backend,
        'build_args': (OUT / 'args.gn').read_text(),
        'chromium_commit': subprocess.check_output(['git', '-C', str(ROOT / 'chromium/src'), 'rev-parse', 'HEAD'], text=True).strip(),
        'graphics_symbols': ['hhalf', 'vhalf', 'quad', 'sextant'],
        'windows': 'independent terminal windows through xdg-run or xdg-terminal-exec; shared live profile',
        'native_dialogs': 'Carbonyl terminal frames; org.freedesktop.portal.FileChooser on the session bus',
        'clipboard': 'standard OSC 52 text clipboard queries and writes; bracketed paste',
        'portal_dependencies': ['python3', 'python3-dbus', 'python3-gi'],
        'window_launcher': 'xdg-terminal-exec or a configured xdg-run',
        'profile': '$XDG_DATA_HOME/carbonyl/profile (default: ~/.local/share/carbonyl/profile)',
        'sha256': {name: hashlib.file_digest((STAGE / name).open('rb'), 'sha256').hexdigest()
                   for name in (('carbonyl', 'libcarbonyl.so', 'carbonyl-file-portal.py') if args.backend == 'chrome' else ('carbonyl', 'libcarbonyl.so'))}}
    (STAGE / 'build-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print('Staged:', STAGE)
    raise SystemExit()

assert (STAGE / 'build-manifest.json').exists(), 'Stage first'
manifest = json.loads((STAGE / 'build-manifest.json').read_text())
assert manifest['backend'] == args.backend, 'Stage the selected backend first'
for name, expected in manifest['sha256'].items():
    assert hashlib.file_digest((STAGE / name).open('rb'), 'sha256').hexdigest() == expected, 'Staged file changed: ' + name
if args.require_tested:
    assert (ROOT / 'build/integration-passed.json').exists(), 'Run the local browser integration test first'
    tested = json.loads((ROOT / 'build/integration-passed.json').read_text())
    assert Path(tested['binary']).resolve() == (STAGE / 'carbonyl').resolve(), 'Test the selected backend first'
    for name in ('carbonyl', 'libcarbonyl.so'):
        current = hashlib.file_digest((STAGE / name).open('rb'), 'sha256').hexdigest()
        assert tested.get('sha256', {}).get(name) == current, 'Test this exact build first: ' + name
    if args.backend == 'chrome':
        receipt = json.loads((ROOT / 'build/file-transfer-passed.json').read_text())
        assert Path(receipt['binary']).resolve() == (STAGE / 'carbonyl').resolve(), 'Test file transfers first'
        for name in ('carbonyl', 'libcarbonyl.so', 'carbonyl-file-portal.py'):
            current = hashlib.file_digest((STAGE / name).open('rb'), 'sha256').hexdigest()
            assert receipt['sha256'][name] == current, 'Test file transfers with this exact build: ' + name
stamp = datetime.now().strftime('%Y%m%d-%H%M%S')
# Replacing mapped libraries by pathname keeps running browsers on their old
# inode. Never truncate a library that a currently open browser is using.
def atomic_copy(source, destination):
    destination = Path(destination)
    if destination.name in ('carbonyl', 'libcarbonyl.so', 'build-manifest.json') and destination.exists():
        shutil.copy2(destination, destination.with_name(destination.name + '.before-' + stamp))
    temporary = destination.with_name(destination.name + '.new-' + stamp)
    shutil.copy2(source, temporary)
    os.replace(temporary, destination)
    return str(destination)
shutil.copytree(STAGE, DEST, dirs_exist_ok=True, copy_function=atomic_copy)
for entry in (Path('/usr/local/bin/carbonyl'), Path.home() / 'carbonyl'):
    if entry.exists() and not entry.is_symlink():
        raise RuntimeError(f'Refusing to replace a regular file: {entry}')
    if entry.is_symlink():
        entry.with_name(entry.name + '.before-' + stamp).symlink_to(os.readlink(entry))
    temporary = entry.with_name(entry.name + '.new-' + stamp)
    temporary.symlink_to(DEST / 'carbonyl')
    os.replace(temporary, entry)
print('Installed:', DEST)
print('Previous runtime and launcher backups have been preserved.')
