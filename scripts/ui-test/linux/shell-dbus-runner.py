#!/usr/bin/env python3
# Runs a command in the mocked D-Bus environment of gnome-shell's own tests:
# private session and system buses with python-dbusmock services for logind,
# AccountsService, GDM, polkit, UPower and so on. It's gnome-shell's
# tests/gnome-shell-dbus-runner.py.in, with mutter_dbusrunner.py from
# Fedora's mutter-tests package and gnomeshell_dbusrunner.py (with
# dbusmock-templates/) copied from gnome-shell's tests.
#
#   shell-dbus-runner.py [--no-isolate-dirs] -- COMMAND...

import glob
import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
mutter_tests = sorted(glob.glob('/usr/share/mutter-*/tests'))
if not mutter_tests:
    sys.exit('shell-dbus-runner: no /usr/share/mutter-*/tests (dnf install mutter-tests)')
sys.path.insert(1, mutter_tests[-1])
sys.path.insert(2, here)

from mutter_dbusrunner import meta_run  # noqa: E402
from gnomeshell_dbusrunner import GnomeShellDBusRunner  # noqa: E402

if __name__ == '__main__':
    sys.exit(meta_run(GnomeShellDBusRunner))
