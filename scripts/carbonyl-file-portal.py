#!/usr/bin/python3
"""Bridge Chromium's native file dialogs to the session FileChooser portal."""
import ctypes
import json
import os
from pathlib import Path
import signal
import sys
import uuid
if len(sys.argv) > 1 and sys.argv[1] == '--open-window':
    import shutil
    import subprocess
    _, _, window_id, profile, binary = sys.argv
    if not window_id.isdecimal() or int(window_id) <= 0:
        raise SystemExit('Carbonyl: invalid window id')
    environment = os.environ.copy()
    environment.pop('CARBONYL_ENV_SHELL_MODE', None)
    environment.pop('CARBONYL_SESSION_SOCKET', None)
    command = [binary, '--user-data-dir=' + profile,
               '--carbonyl-window-id=' + window_id]
    launcher = shutil.which('xdg-run') or shutil.which('xdg-terminal-exec')
    if not launcher:
        raise SystemExit('Carbonyl: install xdg-terminal-exec or configure xdg-run to launch terminal applications')
    sys.exit(subprocess.call([launcher, '--', *command], env=environment))

import dbus
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

DESKTOP = '/org/freedesktop/portal/desktop'
CHOOSER = 'org.freedesktop.portal.FileChooser'
REQUEST = 'org.freedesktop.portal.Request'

def choose(packet, parent):
    # A dead browser must not leave a file dialog or portal request behind.
    ctypes.CDLL(None).prctl(1, signal.SIGTERM, 0, 0, 0)
    if os.getppid() != parent:
        return {'response': 1, 'uris': []}
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus(private=True)
    loop = GLib.MainLoop()
    token = 'carbonyl' + uuid.uuid4().hex
    owner = bus.get_unique_name().lstrip(':').replace('.', '_')
    handle = f'{DESKTOP}/request/{owner}/{token}'
    result = {'response': 1, 'uris': []}
    options = {'handle_token': dbus.String(token),
               'multiple': dbus.Boolean(packet['multiple']),
               'directory': dbus.Boolean(packet['directory'])}
    path = Path(packet['path']) if packet.get('path') else None
    if path:
        folder = path if path.is_dir() else path.parent
        if folder.is_absolute():
            options['current_folder'] = dbus.ByteArray(os.fsencode(folder) + b'\0')
        if packet['save']:
            options['current_name'] = dbus.String(path.name)
        elif path.is_file():
            options['current_file'] = dbus.ByteArray(os.fsencode(path) + b'\0')
    filters = packet.get('filters', [])
    if filters:
        def typed_filter(item):
            return dbus.Struct((dbus.String(item[0]), dbus.Array([
                dbus.Struct((dbus.UInt32(k), dbus.String(p)), signature='us')
                for k, p in item[1]], signature='(us)')), signature='sa(us)')
        options['filters'] = dbus.Array([typed_filter(f) for f in filters], signature='(sa(us))')
        index = packet.get('filter_index', 0)
        options['current_filter'] = typed_filter(filters[max(0, min(index - 1, len(filters) - 1))])
    def response(code, values):
        result['response'] = int(code)
        result['uris'] = [str(uri) for uri in values.get('uris', [])] if code == 0 else []
        current = values.get('current_filter')
        if current:
            normalized = [str(current[0]), [[int(k), str(p)] for k, p in current[1]]]
            if normalized in filters:
                result['filter_index'] = filters.index(normalized) + 1
        loop.quit()
    bus.add_signal_receiver(response, signal_name='Response', dbus_interface=REQUEST,
                            path=handle, bus_name='org.freedesktop.portal.Desktop')
    def close(*_):
        try:
            bus.get_object('org.freedesktop.portal.Desktop', handle, introspect=False).Close(
                dbus_interface=REQUEST, timeout=2)
        except dbus.DBusException:
            pass
        loop.quit()
        return False
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, close)
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGINT, close)
    try:
        proxy = bus.get_object('org.freedesktop.portal.Desktop', DESKTOP, introspect=False)
        method = proxy.get_dbus_method('SaveFile' if packet['save'] else 'OpenFile', CHOOSER)
        returned = str(method('', packet['title'], dbus.Dictionary(options, signature='sv'), timeout=30))
        if returned != handle:
            bus.add_signal_receiver(response, signal_name='Response', dbus_interface=REQUEST,
                                    path=returned, bus_name='org.freedesktop.portal.Desktop')
            handle = returned
        loop.run()
    finally:
        bus.close()
    return result

if __name__ == '__main__':
    try:
        result = choose(json.loads(sys.argv[1]), int(sys.argv[2]))
    except Exception as error:
        print(f'Carbonyl: portal wyboru plików: {error}', file=sys.stderr)
        result = {'response': 2, 'uris': []}
    print(json.dumps(result, ensure_ascii=True))
