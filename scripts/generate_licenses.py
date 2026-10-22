"""Bundle dependency license notices for redistribution of the WASM binary."""

import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1] / "regoli-core"


def main():
    """Collect licenses for resolved WASM dependencies into the bundled notice file."""
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked",
         "--filter-platform", "wasm32-unknown-unknown"],
        cwd=ROOT, check=True, capture_output=True, text=True, encoding="utf-8",
    )
    metadata = json.loads(result.stdout)
    resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
    sections = [
        "THIRD-PARTY LICENSE NOTICES",
        "Generated from Cargo.lock by scripts/generate_licenses.py.",
        "regoli source is AGPL-3.0-only licensed. Its bundled WASM contains third-party code.",
    ]
    # Cargo metadata includes the full dependency graph; only emit packages
    # reachable for the actual WebAssembly target.
    for package in sorted(metadata["packages"], key=lambda item: item["name"]):
        if package["id"] not in resolved or package["name"] == "regoli":
            continue
        directory = Path(package["manifest_path"]).parent
        notices = sorted({
            path for pattern in ["LICENSE*", "LICENCE*", "COPYING*", "NOTICE*"]
            for path in directory.glob(pattern) if path.is_file()
        })
        if package.get("license_file"):
            notices = sorted(set(notices) | {directory / package["license_file"]})
        if not notices:
            raise RuntimeError(f"No license notice found for {package['name']}")
        sections += [
            "\n" + "=" * 72,
            f"{package['name']} {package['version']}",
            f"License: {package.get('license') or 'see license text below'}",
            f"Source: {package.get('repository') or package['source']}",
        ]
        for notice in notices:
            sections += [f"\n--- {notice.name} ---", notice.read_text(encoding="utf-8")]
    (ROOT / "THIRD-PARTY-LICENSES.txt").write_text(
        "\n".join(sections) + "\n", encoding="utf-8", newline="\n",
    )
    print("Updated THIRD-PARTY-LICENSES.txt.")


if __name__ == "__main__":
    main()
