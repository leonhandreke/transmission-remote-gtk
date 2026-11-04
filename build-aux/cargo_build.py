#!/usr/bin/env python3
"""Build the executable with cargo, for meson, and copy it to the output.

  cargo_build.py SOURCE_ROOT TARGET_DIR PROFILE OUTPUT [CARGO_ARGS...]

The configuration (TRG_LOCALEDIR, TRG_DATADIR, BLUEPRINT_COMPILER) comes
from the environment meson sets. The executable's path depends on the
profile and on a cargo target (CARGO_BUILD_TARGET) if there's one.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path


def main():
    source_root, target_dir, profile, output, *cargo_args = sys.argv[1:]
    cargo = os.environ.get("CARGO", "cargo")

    cmd = [cargo, "build", "--manifest-path", str(Path(source_root) / "Cargo.toml"),
           "--target-dir", target_dir, *cargo_args]
    if profile == "release":
        cmd.append("--release")
    subprocess.run(cmd, check=True)

    exe_dir = Path(target_dir)
    target = os.environ.get("CARGO_BUILD_TARGET")
    if target:
        exe_dir /= target
    exe_dir /= profile
    name = Path(output).name
    exe = exe_dir / name
    if not exe.exists() and exe.with_suffix(".exe").exists():
        exe = exe.with_suffix(".exe")

    # Replace the output rather than overwriting it, as macOS kills processes
    # whose (signed) executable changed in place.
    tmp = Path(output).with_name(f".{name}.tmp")
    shutil.copy2(exe, tmp)
    os.replace(tmp, output)


if __name__ == "__main__":
    main()
