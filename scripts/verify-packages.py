#!/usr/bin/env python3
"""Inspect actual installers without launching the desktop GUI."""
import os
import plistlib
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "src-tauri/target/release/bundle"


def run(*args, **kwargs):
    result = subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, **kwargs)
    print(result, end="", flush=True)
    return result


def one(pattern):
    paths = list(BUNDLE.glob(pattern))
    assert len(paths) == 1, f"Expected one {pattern}, found {paths}; clean stale bundles"
    assert "0.1.0" in paths[0].name, paths[0]
    return paths[0]


def binaries(directory, mac=False):
    app, codec = directory / "zapbeam", directory / "jpegtran"
    for binary in (app, codec):
        assert binary.is_file() and os.access(binary, os.X_OK), binary
        info = run("file", str(binary))
        assert ("arm64" if mac else "x86-64") in info, info
        deps = run("otool", "-L", str(binary)) if mac else run("ldd", str(binary))
        assert "not found" not in deps, deps
        if binary == codec:
            assert not any(s in deps.lower() for s in ("libjpeg", "libpng", "homebrew", "/usr/local/")), deps
            if mac:
                assert all(line.strip().startswith(("/usr/lib/", "/System/Library/"))
                           for line in deps.splitlines()[1:]), deps
    # Empty PATH proves the packaged codec executes without global utilities.
    output = run(str(codec), "-version", env={**os.environ, "PATH": ""})
    assert "4.1.5" in output, output


def notices(directory):
    for name in ("MOZJPEG.md", "IJG.txt", "RUST.txt"):
        assert list(directory.rglob(name)), f"Missing bundled notice: {name}"


with tempfile.TemporaryDirectory(prefix="zapbeam-package-") as temp:
    temp = Path(temp)
    if sys.platform == "darwin":
        dmg = one("dmg/*.dmg")
        mount = temp / "mount"
        run("hdiutil", "attach", str(dmg), "-readonly", "-nobrowse", "-mountpoint", str(mount))
        try:
            app = mount / "Zapbeam.app"
            info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
            assert info["CFBundleShortVersionString"] == "0.1.0"
            assert info["CFBundleIdentifier"] == "io.github.svenfinger.zapbeam"
            assert info["CFBundleName"] == "Zapbeam"
            assert info["LSMinimumSystemVersion"] == "11.0"
            binaries(app / "Contents/MacOS", mac=True)
            notices(app)
        finally:
            run("hdiutil", "detach", str(mount))
    elif sys.platform.startswith("linux"):
        deb = one("deb/*.deb")
        assert run("dpkg-deb", "-f", str(deb), "Version").strip() == "0.1.0"
        assert run("dpkg-deb", "-f", str(deb), "Architecture").strip() == "amd64"
        assert run("dpkg-deb", "-f", str(deb), "Package").strip() == "zapbeam"
        run("dpkg-deb", "-I", str(deb))
        extracted = temp / "deb"
        run("dpkg-deb", "-x", str(deb), str(extracted))
        binaries(extracted / "usr/bin")
        notices(extracted)
        appimage = one("appimage/*.AppImage")
        assert "x86-64" in run("file", str(appimage))
        subprocess.run([str(appimage), "--appimage-extract"], cwd=temp,
                       stdout=subprocess.DEVNULL, check=True)
        extracted = temp / "squashfs-root"
        binaries(extracted / "usr/bin")
        notices(extracted)
    else:
        raise RuntimeError("Unsupported packaging host")
print("Installer structure, version, architectures, codec and notices verified.")
