#!/usr/bin/python3
"""Input, windows, screenshots and accessibility for the Linux UI tests.

The counterpart of trgui.swift. It runs inside the session that session.sh
starts (it reads the bus addresses from $TRG_SESSION_ENV) and talks to the
headless GNOME Shell over D-Bus:

- org.gnome.Shell.Eval (allowed in unsafe mode, which automation.js turns
  on) for the window list (Meta windows: their frame rects, without the
  client-side shadows), activating windows, and input: a Clutter virtual
  pointer and keyboard inside the compositor, so events are real input to
  the app, with absolute coordinates.
- org.gnome.Shell.Screenshot for screenshots.
- AT-SPI (Atspi) for the app's widget tree: GTK 4 exports it on Linux.
- org.gtk.Actions on the app's bus name to activate its actions.

Coordinates are logical pixels relative to a window's frame (its visible
top left corner, the header bar included).
"""

import argparse
import json
import os
import sys
import time
import warnings

# Atspi's per-interface methods on Accessible are deprecated, but they're
# what the Python bindings offer
warnings.filterwarnings('ignore', category=DeprecationWarning)


def load_session_env():
    path = os.environ.get('TRG_SESSION_ENV') or f'/run/user/{os.getuid()}/trg-session.env'
    try:
        with open(path) as f:
            for line in f:
                key, sep, value = line.rstrip('\n').partition('=')
                if sep:
                    os.environ[key] = value
    except FileNotFoundError:
        sys.exit(f'trgui: no session ({path}); start it with uitest linux up')


load_session_env()

import gi  # noqa: E402

gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, Gio, GLib  # noqa: E402

APP_ID = 'io.github.TransmissionRemoteGtk'

# Meta.WindowType
WINDOW_TYPES = ['normal', 'desktop', 'dock', 'dialog', 'modal-dialog', 'toolbar', 'menu',
                'utility', 'splash', 'dropdown-menu', 'popup-menu', 'tooltip', 'notification',
                'combo', 'dnd', 'override-other']


def die(msg):
    print(f'trgui: {msg}', file=sys.stderr)
    sys.exit(1)


# --- the shell ------------------------------------------------------------

_bus = None


def bus():
    global _bus
    if _bus is None:
        _bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    return _bus


def call(dest, path, iface, method, args, reply='(bs)'):
    return bus().call_sync(dest, path, iface, method, args, GLib.VariantType(reply),
                           Gio.DBusCallFlags.NONE, 10000, None).unpack()


# Creates the virtual devices once; they live as long as the shell
PRELUDE = '''
const C = imports.gi.Clutter, G = imports.gi.GLib;
if (!globalThis.__trg) {
    const seat = global.stage.context.get_backend().get_default_seat();
    globalThis.__trg = {
        ptr: seat.create_virtual_device(C.InputDeviceType.POINTER_DEVICE),
        kbd: seat.create_virtual_device(C.InputDeviceType.KEYBOARD_DEVICE),
    };
}
const T = globalThis.__trg;
const now = () => G.get_monotonic_time();
'''


def shell_eval(code):
    ok, result = call('org.gnome.Shell', '/org/gnome/Shell', 'org.gnome.Shell', 'Eval',
                      GLib.Variant('(s)', (PRELUDE + code,)))
    if not ok:
        die(f'Eval failed (is the shell in unsafe mode?): {result}')
    return json.loads(result) if result else None


def js(value):
    return json.dumps(value)


# --- windows --------------------------------------------------------------

def windows(pid=None):
    wins = shell_eval('''
        JSON.parse(JSON.stringify(global.get_window_actors().map(a => a.meta_window).map(w => {
            const r = w.get_frame_rect();
            return {id: w.get_id(), pid: w.get_pid(), type: w.get_window_type(),
                    title: w.get_title(), app: w.get_gtk_application_id() ?? w.get_wm_class(),
                    x: r.x, y: r.y, w: r.width, h: r.height, focus: w.has_focus(),
                    scale: global.display.get_monitor_scale(w.get_monitor()),
                    transient: w.get_transient_for()?.get_id() ?? null};
        })))''')
    return [w for w in wins if pid is None or w['pid'] == pid]


def window(wid):
    for w in windows():
        if w['id'] == wid:
            return w
    die(f'no window {wid}')


def main_window(pid):
    normal = [w for w in windows(pid) if w['type'] == 0]
    return max(normal, key=lambda w: w['w'] * w['h']) if normal else None


def type_name(t):
    return WINDOW_TYPES[t] if 0 <= t < len(WINDOW_TYPES) else str(t)


def point(x, y, win, px):
    x, y = float(x), float(y)
    if win is None:
        return x, y
    w = window(win)
    if px:
        x, y = x / w['scale'], y / w['scale']
    return w['x'] + x, w['y'] + y


# --- input ----------------------------------------------------------------

BUTTONS = {'left': 1, 'middle': 2, 'right': 3}

KEY_NAMES = {
    'return': 'Return', 'enter': 'Return', 'escape': 'Escape', 'esc': 'Escape', 'tab': 'Tab',
    'space': 'space', 'backspace': 'BackSpace', 'delete': 'Delete', 'del': 'Delete',
    'insert': 'Insert', 'home': 'Home', 'end': 'End', 'pageup': 'Page_Up',
    'page_up': 'Page_Up', 'pagedown': 'Page_Down', 'page_down': 'Page_Down', 'up': 'Up',
    'down': 'Down', 'left': 'Left', 'right': 'Right', 'menu': 'Menu',
    'plus': 'plus', 'minus': 'minus',
}
# cmd is the primary modifier: Command on macOS, Control here
MODIFIERS = {'ctrl': 'Control_L', 'control': 'Control_L', 'cmd': 'Control_L',
             'primary': 'Control_L', 'shift': 'Shift_L', 'alt': 'Alt_L', 'opt': 'Alt_L',
             'option': 'Alt_L', 'super': 'Super_L', 'meta': 'Super_L'}


def keyval_js(name):
    """A JS expression for the keyval of a key name or a single character"""
    low = name.lower()
    if low in MODIFIERS:
        return f'C.KEY_{MODIFIERS[low]}'
    if low in KEY_NAMES:
        return f'C.KEY_{KEY_NAMES[low]}'
    if len(low) >= 2 and low[0] == 'f' and low[1:].isdigit():
        return f'C.KEY_F{low[1:]}'
    if len(name) == 1:
        # letters in combos are lower case (shift+a, not A)
        return f'C.unicode_to_keysym({ord(low)})'
    return f'(C[{js("KEY_" + name)}] ?? (() => {{ throw new Error({js("unknown key " + name)}); }})())'


def move(x, y):
    shell_eval(f'T.ptr.notify_absolute_motion(now(), {x}, {y}); 0')


def button(b, pressed):
    state = 'PRESSED' if pressed else 'RELEASED'
    shell_eval(f'T.ptr.notify_button(now(), {b}, C.ButtonState.{state}); 0')


def keys(names, pressed):
    state = 'PRESSED' if pressed else 'RELEASED'
    shell_eval(''.join(f'T.kbd.notify_keyval(now(), {keyval_js(n)}, C.KeyState.{state});'
                       for n in names) + '0')


def click(x, y, b, count, mods):
    move(x, y)
    time.sleep(0.05)
    keys(mods, True)
    # about as long as a person holds the button
    for _ in range(count):
        button(b, True)
        time.sleep(0.1)
        button(b, False)
        time.sleep(0.05)
    keys(list(reversed(mods)), False)


def drag(x1, y1, x2, y2, steps=20):
    move(x1, y1)
    time.sleep(0.05)
    button(1, True)
    time.sleep(0.1)
    for i in range(1, steps + 1):
        move(x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps)
        time.sleep(0.015)
    time.sleep(0.1)
    button(1, False)


def scroll(dy, dx):
    def steps(n, neg, pos):
        return [neg] * int(-n) if n < 0 else [pos] * int(n)
    for d in steps(dy, 'UP', 'DOWN') + steps(dx, 'LEFT', 'RIGHT'):
        shell_eval(f'T.ptr.notify_discrete_scroll(now(), C.ScrollDirection.{d}, '
                   f'C.ScrollSource.WHEEL); 0')
        time.sleep(0.03)


def key_combo(combo):
    parts = combo.split('+') if combo != '+' else ['plus']
    if parts[-1] == '':  # ctrl++
        parts = parts[:-2] + ['plus']
    keys(parts, True)
    time.sleep(0.02)
    keys(list(reversed(parts)), False)
    time.sleep(0.03)


def type_text(text):
    for ch in text:
        k = f'C.unicode_to_keysym({ord(ch)})'
        shell_eval(f'T.kbd.notify_keyval(now(), {k}, C.KeyState.PRESSED);'
                   f'T.kbd.notify_keyval(now(), {k}, C.KeyState.RELEASED); 0')
        time.sleep(0.02)


# --- screenshots ----------------------------------------------------------

def screenshot(path, rect=None):
    path = os.path.abspath(path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if rect:
        x, y, w, h = (int(round(v)) for v in rect)
        args = GLib.Variant('(iiiibs)', (x, y, w, h, False, path))
        ok, used = call('org.gnome.Shell.Screenshot', '/org/gnome/Shell/Screenshot',
                        'org.gnome.Shell.Screenshot', 'ScreenshotArea', args)
    else:
        args = GLib.Variant('(bbs)', (False, False, path))
        ok, used = call('org.gnome.Shell.Screenshot', '/org/gnome/Shell/Screenshot',
                        'org.gnome.Shell.Screenshot', 'Screenshot', args)
    if not ok:
        die('the screenshot failed')
    return used


# --- accessibility --------------------------------------------------------

TOPLEVEL_ROLES = {Atspi.Role.FRAME, Atspi.Role.WINDOW, Atspi.Role.DIALOG,
                  Atspi.Role.ALERT, Atspi.Role.FILE_CHOOSER}


def atspi_app(pid):
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app is not None and app.get_process_id() == pid:
            return app
    die(f'no accessible application with pid {pid} (is the accessibility bus running?)')


def label(acc):
    """The accessible name, else the text of what labels it (popover menu items)"""
    name = acc.get_name()
    if name:
        return name
    for rel in acc.get_relation_set():
        if rel.get_relation_type() != Atspi.RelationType.LABELLED_BY:
            continue
        for i in range(rel.get_n_targets()):
            t = rel.get_target(i)
            text = t.get_name()
            if not text and 'Text' in t.get_interfaces():
                text = Atspi.Text.get_text(t, 0, -1)
            if text:
                return text
    return ''


def role_name(acc):
    return acc.get_role_name().replace(' ', '-')


def states(acc):
    s = acc.get_state_set()
    names = []
    for st in (Atspi.StateType.SHOWING, Atspi.StateType.FOCUSED, Atspi.StateType.SELECTED,
               Atspi.StateType.CHECKED, Atspi.StateType.INDETERMINATE,
               Atspi.StateType.SENSITIVE, Atspi.StateType.EXPANDED):
        if s.contains(st):
            names.append(st.value_nick)
    return names


def extents(acc):
    """x, y, w, h relative to the accessible's window (logical pixels)"""
    try:
        e = acc.get_extents(Atspi.CoordType.WINDOW)
        return e.x, e.y, e.width, e.height
    except GLib.Error:
        return None


def toplevel_of(acc):
    a = acc
    while a is not None:
        if a.get_role() in TOPLEVEL_ROLES:
            return a
        a = a.get_parent()
    return None


def showing(acc):
    return acc.get_state_set().contains(Atspi.StateType.SHOWING)


def walk(acc, depth=0, max_depth=99, hidden=False):
    """The accessible and its descendants. Unless hidden, it doesn't go into
    the ones that aren't showing (every call is a D-Bus round trip, and the
    hidden notebook pages and list rows are most of the tree)."""
    yield acc, depth
    if depth >= max_depth:
        return
    for i in range(acc.get_child_count()):
        child = acc.get_child_at_index(i)
        if child is not None and (hidden or showing(child)):
            yield from walk(child, depth + 1, max_depth, hidden)


def matches(acc, role, name, exact):
    if role and role_name(acc) != role:
        return False
    if name is not None:
        n = label(acc)
        if exact and n != name:
            return False
        if not exact and name.lower() not in n.lower():
            return False
    return True


def inside(acc, text):
    """Whether an ancestor's label contains text (for example a table row's)"""
    a = acc.get_parent()
    while a is not None and a.get_role() != Atspi.Role.APPLICATION:
        if text.lower() in label(a).lower():
            return True
        a = a.get_parent()
    return False


def find(pid, role, name, exact, hidden=False, within=None):
    return [acc for acc, depth in walk(atspi_app(pid), hidden=hidden)
            if depth > 0 and matches(acc, role, name, exact)
            and (within is None or inside(acc, within))]


def meta_window_for(pid, top):
    """The Meta window of a toplevel accessible, matched by title, else the main one"""
    title = top.get_name() if top is not None else None
    wins = [w for w in windows(pid) if w['type'] in (0, 3, 4)]
    for w in wins:
        if title and w['title'] == title:
            return w
    return main_window(pid)


def describe(acc):
    e = extents(acc)
    ext = f'{e[0]},{e[1]} {e[2]}x{e[3]}' if e else '-'
    text = ''
    if acc.get_role() in (Atspi.Role.TEXT, Atspi.Role.ENTRY, Atspi.Role.PASSWORD_TEXT,
                          Atspi.Role.SPIN_BUTTON) and 'Text' in acc.get_interfaces():
        text = '\ttext=' + json.dumps(Atspi.Text.get_text(acc, 0, -1))
    return f'{role_name(acc)}\t"{label(acc)}"\t{ext}\t{",".join(states(acc))}{text}'


# --- commands -------------------------------------------------------------

def cmd_check(_):
    ok = True
    try:
        unsafe = shell_eval('global.context.unsafe_mode')
        print(f'shell:   unsafe mode {"on" if unsafe else "OFF"}')
        ok &= bool(unsafe)
        print(f'overview: {"visible" if shell_eval("Main.overview.visible") else "hidden"}')
    except GLib.Error as e:
        print(f'shell:   not reachable: {e.message}')
        ok = False
    try:
        Atspi.get_desktop(0).get_child_count()
        print('a11y:    ok')
    except GLib.Error as e:
        print(f'a11y:    {e.message}')
        ok = False
    sys.exit(0 if ok else 1)


def cmd_windows(a):
    for w in windows(a.pid):
        print(f'{w["id"]}\tpid={w["pid"]}\ttype={type_name(w["type"])}\t'
              f'{w["x"]},{w["y"]} {w["w"]}x{w["h"]}\tscale={w["scale"]:g}\t{w["app"] or "-"}\t'
              f'"{w["title"] or ""}"' + ('\tfocus' if w['focus'] else ''))


def cmd_win(a):
    w = main_window(a.pid)
    if w:
        print(w['id'])


def cmd_frontmost(a):
    w = main_window(a.pid)
    if not w:
        die('the app has no window')
    shell_eval(f'''Main.overview.hide();
        const w = global.get_window_actors().map(a => a.meta_window).find(w => w.get_id() == {w["id"]});
        Main.activateWindow(w); 0''')


def cmd_click(a):
    x, y = point(a.x, a.y, a.win, a.px)
    b = BUTTONS['right' if a.right else 'middle' if a.middle else 'left']
    mods = [m for m in (a.mods or '').split(',') if m]
    click(x, y, b, a.count, mods)


def cmd_move(a):
    move(*point(a.x, a.y, a.win, a.px))


def cmd_drag(a):
    x1, y1 = point(a.x1, a.y1, a.win, a.px)
    x2, y2 = point(a.x2, a.y2, a.win, a.px)
    drag(x1, y1, x2, y2)


def cmd_scroll(a):
    if a.at:
        move(*point(a.at[0], a.at[1], a.win, a.px))
        time.sleep(0.05)
    scroll(float(a.dy), float(a.dx))


def cmd_key(a):
    for combo in a.combos:
        key_combo(combo)


def cmd_type(a):
    type_text(a.text)


def cmd_shot(a):
    if a.win:
        w = window(a.win)
        rect = (w['x'], w['y'], w['w'], w['h'])
    elif a.pid:
        wins = [w for w in windows(a.pid) if w['w'] > 1 and w['h'] > 1]
        if not wins:
            die('the app has no window on screen')
        x1 = min(w['x'] for w in wins)
        y1 = min(w['y'] for w in wins)
        x2 = max(w['x'] + w['w'] for w in wins)
        y2 = max(w['y'] + w['h'] for w in wins)
        rect = (x1, y1, x2 - x1, y2 - y1)
        print(f'region {x1},{y1},{x2 - x1},{y2 - y1}')
    else:
        rect = None
    path = screenshot(a.file, rect)
    with open(path, 'rb') as f:
        header = f.read(24)
    width, height = int.from_bytes(header[16:20], 'big'), int.from_bytes(header[20:24], 'big')
    print(f'{path} ({width}x{height} px)')


def cmd_ax(a):
    for acc, depth in walk(atspi_app(a.pid), max_depth=a.depth, hidden=a.all):
        print('  ' * depth + describe(acc))


def cmd_find(a):
    found = find(a.pid, a.role, a.name, a.exact, hidden=a.hidden, within=a.inside)
    if not found:
        die(f'nothing found (role={a.role}, name={a.name})')
    for acc in found if a.all else found[:1]:
        target = acc
        if a.cell:
            # GTK 4.24 places the widgets in column view cells at the cell's
            # corner, though it draws them centered: use the cell's center
            target = acc.get_parent()
            while target is not None and target.get_role() != Atspi.Role.TABLE_CELL:
                target = target.get_parent()
            if target is None:
                die(f'{describe(acc)} is not in a table cell')
        e = extents(target)
        w = meta_window_for(a.pid, toplevel_of(acc))
        if e and w:
            cx, cy = e[0] + e[2] // 2, e[1] + e[3] // 2
            print(f'{cx} {cy}\twin={w["id"]}\t{describe(acc)}')
        else:
            print(f'- -\twin=-\t{describe(acc)}')


def cmd_press(a):
    found = find(a.pid, a.role, a.name, a.exact, within=a.inside)
    if not found:
        die(f'nothing found (role={a.role}, name={a.name})')
    acc = found[0]
    names = actions(acc)
    want = a.action or (names[0] if names else None)
    if want not in names:
        die(f'{describe(acc)} has the actions {names}, not {want}')
    acc.do_action(names.index(want))
    print(f'{want}: {describe(acc)}')


def actions(acc):
    if not acc.get_action_iface():
        return []
    return [acc.get_action_name(i) for i in range(acc.get_n_actions())]


def wait_find(pid, role, name, exact, timeout=3.0):
    end = time.monotonic() + timeout
    while True:
        found = find(pid, role, name, exact)
        if found or time.monotonic() > end:
            return found
        time.sleep(0.1)


def cmd_menu(a):
    """Open a menu of the menu bar with a click, then press the items by label"""
    w = main_window(a.pid)
    if not w:
        die('the app has no window')
    top = wait_find(a.pid, 'menu-item', a.path[0], True, 0)
    if not top:
        die(f'no menu {a.path[0]!r} in the menu bar')
    e = extents(top[0])
    click(w['x'] + e[0] + e[2] / 2, w['y'] + e[1] + e[3] / 2, 1, 1, [])
    for title in a.path[1:]:
        items = wait_find(a.pid, 'menu-item', title, True)
        if not items:
            die(f'no menu item {title!r}')
        if 'click' not in actions(items[0]):
            die(f'{describe(items[0])} has no click action')
        items[0].do_action(actions(items[0]).index('click'))
        time.sleep(0.3)


def cmd_action(a):
    name = a.name
    prefix, _, short = name.partition('.')
    if not short or prefix not in ('app', 'win'):
        die('the action needs a prefix: app.NAME or win.NAME')
    path = '/' + APP_ID.replace('.', '/')
    if prefix == 'win':
        path += f'/window/{a.window}'
    params = [GLib.Variant.parse(None, a.param, None, None)] if a.param else []
    bus().call_sync(APP_ID, path, 'org.gtk.Actions', 'Activate',
                    GLib.Variant('(sava{sv})', (short, params, {})), None,
                    Gio.DBusCallFlags.NONE, 10000, None)


def cmd_eval(a):
    print(json.dumps(shell_eval(a.code)))


def main():
    p = argparse.ArgumentParser(prog='trgui.py')
    sub = p.add_subparsers(dest='cmd', required=True)

    def add(name, fn, pid=False, win=False):
        s = sub.add_parser(name)
        s.set_defaults(fn=fn)
        if pid:
            s.add_argument('--pid', type=int, required=pid == 'required')
        if win:
            s.add_argument('--win', type=int)
            s.add_argument('--px', action='store_true')
        return s

    add('check', cmd_check)
    add('windows', cmd_windows, pid=True)
    add('win', cmd_win, pid='required')
    add('frontmost', cmd_frontmost, pid='required')
    s = add('click', cmd_click, win=True)
    s.add_argument('x')
    s.add_argument('y')
    s.add_argument('--right', action='store_true')
    s.add_argument('--middle', action='store_true')
    s.add_argument('--count', type=int, default=1)
    s.add_argument('--mods')
    s = add('move', cmd_move, win=True)
    s.add_argument('x')
    s.add_argument('y')
    s = add('drag', cmd_drag, win=True)
    for n in ('x1', 'y1', 'x2', 'y2'):
        s.add_argument(n)
    s = add('scroll', cmd_scroll, win=True)
    s.add_argument('dy')
    s.add_argument('dx', nargs='?', default='0')
    s.add_argument('--at', nargs=2)
    s = add('key', cmd_key)
    s.add_argument('combos', nargs='+')
    s = add('type', cmd_type)
    s.add_argument('text')
    s = add('shot', cmd_shot, pid=True)
    s.add_argument('file')
    s.add_argument('--win', type=int)
    s = add('ax', cmd_ax, pid='required')
    s.add_argument('--depth', type=int, default=99)
    s.add_argument('--all', action='store_true', help='also the widgets that are not showing')
    for name, fn in (('find', cmd_find), ('press', cmd_press)):
        s = add(name, fn, pid='required')
        s.add_argument('--role')
        s.add_argument('--name')
        s.add_argument('--exact', action='store_true')
        s.add_argument('--inside', metavar='TEXT',
                       help="only in a widget whose label contains TEXT, e.g. a table row's")
        if name == 'find':
            s.add_argument('--all', action='store_true')
            s.add_argument('--hidden', action='store_true')
            s.add_argument('--cell', action='store_true',
                           help='the center of the table cell that has the widget')
        else:
            s.add_argument('--action')
    s = add('menu', cmd_menu, pid='required')
    s.add_argument('path', nargs='+')
    s = add('action', cmd_action)
    s.add_argument('name')
    s.add_argument('param', nargs='?')
    s.add_argument('--window', type=int, default=1)
    s = add('eval', cmd_eval)
    s.add_argument('code')

    a = p.parse_args()
    try:
        a.fn(a)
    except GLib.Error as e:
        die(e.message)


if __name__ == '__main__':
    main()
