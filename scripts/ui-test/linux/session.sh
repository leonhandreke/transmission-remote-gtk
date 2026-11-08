#!/bin/bash
# The entry point of the Linux UI test container. It starts a headless GNOME
# Shell the way gnome-shell's own tests do (tests/meson.build and
# gnome-shell-test-tool): in the mocked D-Bus environment of
# shell-dbus-runner.py, with --headless --virtual-monitor, a keyfile
# GSettings backend and an automation script (automation.js) that turns on
# unsafe mode for trgui.py.
#
# The runner creates private buses, so this writes the environment that
# other processes need to join the session (the bus addresses,
# WAYLAND_DISPLAY, XDG_RUNTIME_DIR) to $TRG_SESSION_ENV, and touches
# $TRG_SESSION_ENV.ready when the shell is ready. trgui.py and uitest read
# it.
#
# Environment: TRG_UITEST_MONITOR (default 1280x800),
#              TRG_SESSION_ENV (default /run/user/<uid>/trg-session.env)

set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export TRG_SESSION_ENV=${TRG_SESSION_ENV:-/run/user/$(id -u)/trg-session.env}

if [[ ${1:-} != --in-session ]]; then
    rm -f "$TRG_SESSION_ENV" "$TRG_SESSION_ENV.ready"
    # the mocked logind would otherwise pass calls on to the host's logind
    export META_DBUS_RUNNER_DISABLE_LOGIND_PASSTHROUGH=1
    exec python3 "$here/shell-dbus-runner.py" -- "$0" --in-session
fi

# Inside the runner: HOME, TMPDIR and the XDG dirs are fresh temporary dirs
cat >"$TRG_SESSION_ENV" <<EOF
DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS
DBUS_SYSTEM_BUS_ADDRESS=$DBUS_SYSTEM_BUS_ADDRESS
XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR
WAYLAND_DISPLAY=wayland-0
XDG_SESSION_TYPE=wayland
XDG_CURRENT_DESKTOP=GNOME
SESSION_HOME=$HOME
EOF

# as gnome-shell-test-tool does: a keyfile GSettings backend
export GSETTINGS_BACKEND=keyfile
mkdir -p "$XDG_CONFIG_HOME/glib-2.0/settings"
cat >"$XDG_CONFIG_HOME/glib-2.0/settings/keyfile" <<'EOF'
[org/gnome/desktop/interface]
enable-animations=false
# The virtual pointer's first motion in a new session can hit the hot
# corner's pressure barrier, which opens the overview
enable-hot-corners=false

[org/gnome/desktop/session]
idle-delay=uint32 0

[org/gnome/desktop/screensaver]
lock-enabled=false

[org/gnome/desktop/notifications]
show-banners=false
EOF

# The mocked session bus doesn't activate services, so start the
# accessibility bus (AT-SPI, for GTK's widget tree) here
/usr/libexec/at-spi-bus-launcher --launch-immediately >/dev/null 2>&1 &

shell_log=$(dirname "$TRG_SESSION_ENV")/trg-shell.log
gnome-shell --headless --wayland --no-x11 --wayland-display wayland-0 \
    --virtual-monitor "${TRG_UITEST_MONITOR:-1280x800}" \
    --automation-script "$here/automation.js" >"$shell_log" 2>&1 &
shell=$!

for _ in $(seq 150); do
    if [[ $(gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell \
        --method org.gnome.Shell.Eval 'globalThis.trgUitestReady === true' 2>/dev/null) == "(true, 'true')" ]]; then
        touch "$TRG_SESSION_ENV.ready"
        echo "GNOME Shell $(gnome-shell --version | grep -oE '[0-9.]+$') is ready (log: $shell_log)"
        break
    fi
    kill -0 $shell 2>/dev/null || { cat "$shell_log"; exit 1; }
    sleep 0.2
done
[[ -f $TRG_SESSION_ENV.ready ]] || { tail -30 "$shell_log"; echo "the shell isn't ready after 30 s"; }
wait $shell
