#!/usr/bin/env python3
"""A keyboard for a Wayland seat that has none, held until this ends.

The desktop sandbox's sway is headless: its seat has no keyboard, and a
compositor gives keyboard focus, and tells a client it gained or lost it,
only on a seat with one. This client gives the seat a virtual keyboard
(zwp_virtual_keyboard_v1) that never types; the compositor then moves the
focus between its clients as on a desktop. It ends when the compositor goes
away or it is killed, and the seat's keyboard goes with it.

It speaks the Wayland wire protocol itself, with Python's standard library
only, so the sandbox needs no package for it. The keymap it gives is the
US layout by name; the compositor compiles it and hands its clients the
result.

    seat-keyboard.py    (WAYLAND_DISPLAY and XDG_RUNTIME_DIR name the compositor)
"""

import os
import socket
import struct
import sys

KEYMAP = b"""xkb_keymap {
  xkb_keycodes { include "evdev+aliases(qwerty)" };
  xkb_types { include "complete" };
  xkb_compat { include "complete" };
  xkb_symbols { include "pc+us+inet(evdev)" };
};
\0"""
XKB_V1 = 1

DISPLAY, REGISTRY, SYNC, SEAT, MANAGER, KEYBOARD = 1, 2, 3, 4, 5, 6


def die(why):
    print(f"seat-keyboard: {why}", file=sys.stderr)
    sys.exit(1)


def string(text):
    data = text.encode() + b"\0"
    return struct.pack("=I", len(data)) + data + b"\0" * (-len(data) % 4)


def request(sock, obj, opcode, body=b"", fds=()):
    head = struct.pack("=II", obj, (8 + len(body)) << 16 | opcode)
    sock.sendmsg([head + body], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, struct.pack(f"={len(fds)}i", *fds))] if fds else [])


class Events:
    """Messages from the compositor, one at a time."""

    def __init__(self, sock):
        self.sock = sock
        self.buf = b""

    def next(self):
        while True:
            if len(self.buf) >= 8:
                obj, word = struct.unpack_from("=II", self.buf)
                size = word >> 16
                if size < 8:
                    die("the compositor sent a message shorter than its header")
                if len(self.buf) >= size:
                    body, self.buf = self.buf[8:size], self.buf[size:]
                    return obj, word & 0xFFFF, body
            # A compositor sends fds with some events (a keymap to a keyboard
            # bound by a client); this one binds none, and closes any it gets.
            data, ancillary, _, _ = self.sock.recvmsg(4096, socket.CMSG_SPACE(16 * 4))
            for level, kind, fds in ancillary:
                if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                    for (fd,) in struct.iter_unpack("=i", fds[: len(fds) - len(fds) % 4]):
                        os.close(fd)
            if not data:
                return None
            self.buf += data


def read_string(body, at):
    (n,) = struct.unpack_from("=I", body, at)
    text = body[at + 4 : at + 4 + n - 1].decode(errors="replace")
    return text, at + 4 + n + (-n % 4)


def fail_on_error(obj, opcode, body):
    # wl_display.error: the compositor ends the connection after it.
    if obj == DISPLAY and opcode == 0:
        message, _ = read_string(body, 8)
        die(f"the compositor refused a request: {message}")


def main():
    name = os.environ.get("WAYLAND_DISPLAY") or die("WAYLAND_DISPLAY is not set")
    if not name.startswith("/"):
        runtime = os.environ.get("XDG_RUNTIME_DIR") or die("XDG_RUNTIME_DIR is not set")
        name = os.path.join(runtime, name)
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.connect(name)
    except OSError as e:
        die(f"no compositor at {name}: {e}")
    events = Events(sock)

    request(sock, DISPLAY, 1, struct.pack("=I", REGISTRY))  # get_registry
    request(sock, DISPLAY, 0, struct.pack("=I", SYNC))  # sync: done once the globals are told
    globals_ = {}
    while True:
        event = events.next()
        if event is None:
            die("the compositor closed the connection")
        obj, opcode, body = event
        fail_on_error(obj, opcode, body)
        if obj == REGISTRY and opcode == 0:  # global
            (gname,) = struct.unpack_from("=I", body)
            interface, at = read_string(body, 4)
            (version,) = struct.unpack_from("=I", body, at)
            globals_.setdefault(interface, (gname, version))
        elif obj == SYNC and opcode == 0:
            break

    for interface, new_id in (("wl_seat", SEAT), ("zwp_virtual_keyboard_manager_v1", MANAGER)):
        if interface not in globals_:
            die(f"the compositor offers no {interface}")
        gname, _ = globals_[interface]
        request(sock, REGISTRY, 0, struct.pack("=I", gname) + string(interface) + struct.pack("=II", 1, new_id))
    request(sock, MANAGER, 0, struct.pack("=II", SEAT, KEYBOARD))  # create_virtual_keyboard

    keymap = os.memfd_create("seat-keyboard-keymap", os.MFD_CLOEXEC)
    os.write(keymap, KEYMAP)
    request(sock, KEYBOARD, 0, struct.pack("=II", XKB_V1, len(KEYMAP)), fds=(keymap,))
    os.close(keymap)

    # Held from here; the seat's events (its name, what it can do) need no answer.
    while (event := events.next()) is not None:
        fail_on_error(*event)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
