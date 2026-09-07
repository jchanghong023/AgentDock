"""Linux regression: hide XKEYBOARD in X11 QueryExtension replies to a TCP Xvfb.

This exercises the actual no-extension branch, not a production force-fallback
switch. It does not emulate Citrix, CentOS ABI, or an input method server.
All generated files, Cargo downloads and Xvfb logs stay in repository .tmp.
Requires cargo, Xvfb, libX11, libxkbcommon and system XKB data; installs nothing.
"""
import ctypes as C
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
TMP = ROOT / '.tmp' / 'x11-keyboard'
TMP.mkdir(parents=True, exist_ok=True)
os.environ.update(TMPDIR=str(TMP), TMP=str(TMP), TEMP=str(TMP),
                  CARGO_HOME=str(ROOT / '.tmp/linux-cargo'),
                  CARGO_TARGET_DIR=str(ROOT / '.tmp/linux-xkb-target'))


def readn(sock, size):
    data = bytearray()
    while len(data) < size:
        chunk = sock.recv(size - len(data))
        if not chunk:
            raise EOFError
        data.extend(chunk)
    return data


class Proxy:
    def __init__(self, port, backend):
        self.backend = backend
        self.hidden = 0
        self.sock = socket.socket()
        self.sock.bind(('127.0.0.1', port))
        self.sock.listen()
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while True:
            try:
                client, _ = self.sock.accept()
            except OSError:
                return
            threading.Thread(target=self.serve, args=(client,), daemon=True).start()

    def serve(self, client):
        server = socket.create_connection(('127.0.0.1', self.backend))
        pending = set()
        try:
            setup = readn(client, 12)
            endian = '<' if setup[0] == ord('l') else '>'
            n, d = struct.unpack_from(endian + 'HH', setup, 6)
            server.sendall(setup + readn(client, ((n + 3) & ~3) + ((d + 3) & ~3)))
            reply = readn(server, 8)
            client.sendall(reply + readn(server, struct.unpack_from(endian + 'H', reply, 6)[0] * 4))

            def requests():
                seq = 0
                try:
                    while True:
                        req = readn(client, 4)
                        size = struct.unpack_from(endian + 'H', req, 2)[0] * 4
                        if size == 0:
                            req += readn(client, 4)
                            size = struct.unpack_from(endian + 'I', req, 4)[0] * 4
                        req += readn(client, size - len(req))
                        seq = (seq + 1) & 65535
                        if req[0] == 98:
                            length = struct.unpack_from(endian + 'H', req, 4)[0]
                            if req[8:8+length] == b'XKEYBOARD':
                                pending.add(seq)
                        server.sendall(req)
                except (EOFError, OSError):
                    server.close()

            threading.Thread(target=requests, daemon=True).start()
            while True:
                reply = readn(server, 32)
                if reply[0] in (1, 35):
                    reply += readn(server, struct.unpack_from(endian + 'I', reply, 4)[0] * 4)
                seq = struct.unpack_from(endian + 'H', reply, 2)[0]
                if reply[0] == 1 and seq in pending:
                    pending.remove(seq)
                    reply[8:12] = b'\0' * 4
                    self.hidden += 1
                client.sendall(reply)
        except (EOFError, OSError):
            pass
        finally:
            client.close()
            server.close()


class Key(C.Structure):
    _fields_ = [('type', C.c_int), ('serial', C.c_ulong), ('send_event', C.c_int),
                ('display', C.c_void_p), ('window', C.c_ulong), ('root', C.c_ulong),
                ('subwindow', C.c_ulong), ('time', C.c_ulong), ('x', C.c_int),
                ('y', C.c_int), ('x_root', C.c_int), ('y_root', C.c_int),
                ('state', C.c_uint), ('keycode', C.c_uint), ('same_screen', C.c_int)]


def inject(display, window, core_repeat=False):
    lib = C.CDLL('libX11.so.6')
    lib.XOpenDisplay.argtypes = [C.c_char_p]
    lib.XOpenDisplay.restype = C.c_void_p
    lib.XSendEvent.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_long, C.c_void_p]
    lib.XFlush.argtypes = [C.c_void_p]
    lib.XSetInputFocus.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_ulong]
    lib.XCloseDisplay.argtypes = [C.c_void_p]
    conn = lib.XOpenDisplay(display.encode())
    assert conn
    try:
        lib.XSetInputFocus(conn, window, 1, 0)
        lib.XFlush(conn)
        time.sleep(.2)
        # a, Shift+a, Ctrl+c, Enter, Left; evdev X11 keycodes.
        for code, mods in [(38, 0), (38, 1), (54, 4), (36, 0), (113, 0)]:
            for kind in (2, 3):
                event = (C.c_long * 24)()
                key = C.cast(event, C.POINTER(Key)).contents
                key.type, key.display, key.window = kind, conn, window
                key.same_screen, key.keycode, key.state = 1, code, mods
                assert lib.XSendEvent(conn, window, 0, 1 if kind == 2 else 2, event)
                lib.XFlush(conn)
                time.sleep(.05)
        if core_repeat:
            # Send a held 'b', a core repeat release/press pair, then final release.
            for kind, stamp in [(2, 100), (3, 101), (2, 101), (3, 102)]:
                event = (C.c_long * 24)()
                key = C.cast(event, C.POINTER(Key)).contents
                key.type, key.display, key.window = kind, conn, window
                key.same_screen, key.keycode, key.time = 1, 56, stamp
                assert lib.XSendEvent(conn, window, 0, 1 if kind == 2 else 2, event)
            lib.XFlush(conn)
    finally:
        lib.XCloseDisplay(conn)


def run_probe(binary, display, name, bad_root=False):
    env = dict(os.environ, DISPLAY=display)
    env.pop('WAYLAND_DISPLAY', None)
    env['XKB_DEFAULT_LAYOUT'] = 'us'
    env.pop('XKB_CONFIG_ROOT', None)
    if bad_root:
        env['XKB_CONFIG_ROOT'] = str(TMP / 'missing-xkb')
    log = TMP / (name + '.log')
    with log.open('w') as output:
        proc = subprocess.Popen([binary], env=env, stdout=output, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                text = log.read_text()
                if bad_root and proc.poll() is not None:
                    assert proc.returncode == 2 and 'INIT_ERROR' in text and 'XKB_CONFIG_ROOT' in text, text
                    assert 'panicked' not in text, text
                    return
                window = next((line.split()[1] for line in text.splitlines() if line.startswith('WINDOW ')), None)
                if window:
                    time.sleep(.5)
                    inject(display, int(window), core_repeat=name == 'no-xkb')
                    time.sleep(.5)
                    text = log.read_text()
                    for expected in ['Pressed Character("a")', 'Pressed Character("A")',
                                     'Pressed Character("c")', 'Pressed Named(Enter)',
                                     'Pressed Named(ArrowLeft)', 'Released Character("a")',
                                     'CONTROL', 'SHIFT']:
                        assert expected in text, (expected, text)
                    if name == 'no-xkb':
                        assert 'Pressed Character("b") Some("b") repeat=true' in text, text
                        assert text.count('Released Character("b")') == 1, text
                    return
                assert proc.poll() is None, text
                time.sleep(.1)
            raise AssertionError('probe timed out: ' + log.read_text())
        finally:
            if proc.poll() is None:
                proc.terminate()
            proc.wait(timeout=5)


def main():
    crate = TMP / 'probe'
    (crate / 'src').mkdir(parents=True, exist_ok=True)
    (crate / 'Cargo.toml').write_text('[package]\nname="x11-keyboard-probe"\nversion="0.1.0"\nedition="2021"\n'
        '[dependencies]\nwinit={path="' + (ROOT / 'vendor/winit').as_posix() + '",default-features=false,features=["x11","rwh_06"]}\n')
    shutil.copyfile(ROOT / 'scripts/x11_keyboard_probe.rs', crate / 'src/main.rs')
    if not (crate / 'Cargo.lock').exists():
        shutil.copyfile(ROOT / 'vendor/winit/Cargo.lock', crate / 'Cargo.lock')
    subprocess.run(['cargo', 'build', '--offline', '--manifest-path', str(crate / 'Cargo.toml')], check=True)
    binary = str(ROOT / '.tmp/linux-xkb-target/debug/x11-keyboard-probe')
    # TCP only and -nolock keep Xvfb from creating sockets/locks in system /tmp.
    with (TMP / 'xvfb.log').open('w') as log:
        server = subprocess.Popen(['Xvfb', ':198', '-screen', '0', '800x600x24',
            '-ac', '-nolock', '-nolisten', 'unix', '-listen', 'tcp'], stdout=log, stderr=log)
        proxy = None
        try:
            time.sleep(1)
            assert server.poll() is None, 'Xvfb failed; see ' + str(TMP / 'xvfb.log')
            proxy = Proxy(6199, 6198)
            run_probe(binary, '127.0.0.1:198', 'normal')
            run_probe(binary, '127.0.0.1:199', 'no-xkb')
            run_probe(binary, '127.0.0.1:199', 'missing-data', bad_root=True)
            assert proxy.hidden > 0
            print('PASS: normal XKB, masked XKB keyboard/window, missing data error')
        finally:
            if proxy:
                proxy.sock.close()
            server.terminate()
            server.wait(timeout=5)


if __name__ == '__main__':
    main()
