"""Test the runtime-only package layout without installing a global package."""

from pathlib import Path
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = [
    ("typst.toml", "typst.toml"),
    ("src/lib.typ", "src/lib.typ"),
    ("regoli.wasm", "regoli.wasm"),
    ("README.md", "README.md"),
    ("LICENSE", "LICENSE"),
    ("regoli-core/THIRD-PARTY-LICENSES.txt", "THIRD-PARTY-LICENSES.txt"),
]


def main():
    manifest = tomllib.loads((ROOT / "typst.toml").read_text(encoding="utf-8"))["package"]
    assert manifest["name"] == "regoli"
    assert manifest["version"] == "0.1.0"
    assert manifest["entrypoint"] == "src/lib.typ"
    for field in ["authors", "description", "license"]:
        assert manifest[field], f"Missing publishing metadata: {field}"
    output = ROOT / "tests" / "output"
    packages = output / "packages"
    package = packages / "local" / manifest["name"] / manifest["version"]
    package.mkdir(parents=True, exist_ok=True)
    for source, name in RUNTIME:
        destination = package / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / source, destination)
    subprocess.run(
        ["typst", "compile", "--root", str(ROOT), "--package-path", str(packages),
         str(ROOT / "tests" / "package-local.typ"), str(output / "package.pdf")],
        cwd=ROOT, check=True,
    )
    print("Verified manifest and isolated @local package import.")


if __name__ == "__main__":
    main()
