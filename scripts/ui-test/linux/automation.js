// The automation script that gnome-shell runs (--automation-script), as in
// gnome-shell's own tests (tests/shell/*.js, gnome-shell-test-tool). It
// turns on unsafe mode, which allows the org.gnome.Shell.Eval, Introspect
// and Screenshot D-Bus calls that trgui.py uses, and leaves the overview
// once the shell has started. Then globalThis.trgUitestReady is true.
//
// The shell calls run() only once gnome-shell-perf-helper is on the bus,
// and exits when run() returns, so run() never returns. The setup doesn't
// depend on run(), though: the shell's own spawn of the helper can come too
// early in the container.

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

export var METRICS = {};

function ready() {
    Main.overview.hide();
    // The shell shows the overview when the session starts with no window,
    // and may show it again until the first one appears
    global.display.connect('window-created', () => Main.overview.hide());
    globalThis.trgUitestReady = true;
    log('trg-uitest: the shell is ready');
}

export function init() {
    global.context.unsafe_mode = true;
    if (Main.layoutManager._startingUp)
        Main.layoutManager.connect('startup-complete', ready);
    else
        ready();
}

export async function run() {
    await new Promise(() => {});
}
