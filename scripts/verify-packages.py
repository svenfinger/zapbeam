#!/usr/bin/env python3
"""Inspect actual installers without launching the desktop GUI."""
import argparse
import os
import re
import plistlib
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "src-tauri/target/release/bundle"


def run(*args, **kwargs):
    try:
        result = subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, **kwargs)
    except subprocess.CalledProcessError as error:
        print(error.output, end="", flush=True)
        raise
    print(result, end="", flush=True)
    return result


def detach(mount):
    # macOS can briefly retain the executed codec's mapping after it exits.
    for attempt in range(3):
        try:
            run("hdiutil", "detach", str(mount))
            return
        except subprocess.CalledProcessError as error:
            if error.returncode != 16 or attempt == 2:
                raise
            time.sleep(2)


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


def verify_signature(path, identity, team, *, identifier=None, runtime=False):
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(path))
    details = run("codesign", "-dv", "--verbose=4", str(path))
    lines = details.splitlines()
    if f"Authority={identity}" not in lines or f"TeamIdentifier={team}" not in lines:
        raise RuntimeError(f"Unexpected signing identity or team: {path}")
    if "Signature=adhoc" in lines or not any(line.startswith("Timestamp=") for line in lines):
        raise RuntimeError(f"Expected timestamped Developer ID signature: {path}")
    if identifier and f"Identifier={identifier}" not in lines:
        raise RuntimeError(f"Unexpected signed bundle identifier: {path}")
    if runtime and not re.search(r"flags=.*\bruntime\b", details):
        raise RuntimeError(f"Hardened runtime missing: {path}")


def verify_macos_release(app, dmg):
    identity = os.environ.get("APPLE_SIGNING_IDENTITY", "")
    match = re.fullmatch(r"Developer ID Application: .+ \(([A-Z0-9]{10})\)", identity)
    if not match:
        raise RuntimeError("Set APPLE_SIGNING_IDENTITY to the full expected Developer ID Application identity")
    team = match[1]
    if not (app / "Contents/_CodeSignature/CodeResources").is_file():
        raise RuntimeError("Application resource seal is missing")
    verify_signature(app, identity, team, identifier="io.github.svenfinger.zapbeam", runtime=True)
    # Inspect every Mach-O, including future nested executable components.
    for path in (app / "Contents").rglob("*"):
        if path.is_file() and "Mach-O" in run("file", "-b", str(path)):
            verify_signature(path, identity, team, runtime=True)
    verify_signature(dmg, identity, team)
    run("spctl", "--assess", "--type", "execute", "--verbose=4", str(app))
    # Tauri notarizes/staples the app BEFORE enclosing it in the signed DMG.
    # Validate that ticket on the actual distributed copy, not a staging app.
    run("xcrun", "stapler", "validate", str(app))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-signing", action="store_true",
                        help="Require Developer ID signatures, Gatekeeper acceptance and the app's stapled ticket")
    args = parser.parse_args()
    if args.require_signing and sys.platform != "darwin":
        parser.error("--require-signing requires macOS")

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
                if args.require_signing:
                    verify_macos_release(app, dmg)
            finally:
                detach(mount)
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
    if args.require_signing:
        print("Distributed Developer ID signatures, hardened runtime, Gatekeeper and stapled app ticket verified.")


if __name__ == "__main__":
    main()
