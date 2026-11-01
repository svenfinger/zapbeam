"""Collect upstream notices for the codec pipeline from Cargo's locked sources."""

import json
import stat
import subprocess
import tempfile
from pathlib import Path
from typing import TypedDict

CODEC_NAMES = frozenset(
    [
        "quick-xml",
        "memchr",
        "oxipng",
        "libdeflater",
        "libdeflate-sys",
        "image",
        "moxcms",
        "pxfm",
        "zune-core",
        "zune-jpeg",
        "png",
        "fdeflate",
        "miniz_oxide",
        "adler2",
        "simd-adler32",
        "crc32fast",
        "tempfile",
        "xattr",
        "rustix",
        "errno",
        "filetime",
        "getrandom",
        "linux-raw-sys",
        "rgb",
        "bytemuck",
        "num-traits",
        "byteorder-lite",
        "fastrand",
        "flate2",
        "rustc-hash",
    ]
)


class Package(TypedDict):
    name: str
    version: str
    license: str | None
    manifest_path: str


def collect_notices(root: Path, packages: list[Package]) -> str:
    missing = CODEC_NAMES - {package["name"] for package in packages}
    if missing:
        raise RuntimeError(f"Missing packages: {', '.join(sorted(missing))}")

    sections: list[str] = []
    for package in sorted(packages, key=lambda p: (p["name"], p["version"])):
        if package["name"] not in CODEC_NAMES:
            continue
        sections.append(
            f"\n===== {package['name']} {package['version']} "
            f"({package['license']}) =====\n"
        )
        directory = Path(package["manifest_path"]).parent
        notices = sorted(
            path
            for path in directory.rglob("*")
            if path.is_file()
            and path.name.lower().startswith(("license", "licence", "copying"))
        )
        if not notices and package["name"].startswith("zune-"):
            sections.append(
                (root / "docs/licenses/ZUNE-MIT.txt").read_text(encoding="utf-8")
            )
            continue
        if not notices:
            raise RuntimeError(f"Missing notices: {package['name']}")
        for path in notices:
            sections.append(
                f"\n--- {path.relative_to(directory)} ---\n"
                f"{path.read_text(encoding='utf-8')}\n"
            )
    return "".join(sections)


def write_notices(destination: Path, contents: str) -> None:
    mode = stat.S_IMODE(destination.stat().st_mode) if destination.exists() else 0o644
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=destination.parent,
            prefix=f".{destination.name}.",
            delete=False,
        ) as output:
            temporary = Path(output.name)
            output.write(contents)
        temporary.chmod(mode)
        temporary.replace(destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--locked",
                "--offline",
                "--format-version",
                "1",
                "--manifest-path",
                str(root / "src-tauri/Cargo.toml"),
            ],
            text=True,
            encoding="utf-8",
        )
    )
    contents = collect_notices(root, metadata["packages"])
    write_notices(root / "docs/licenses/RUST.txt", contents)


if __name__ == "__main__":
    main()
