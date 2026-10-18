"""Test the runtime-only package layout without installing a global package."""

from pathlib import Path
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = [
    "typst.toml", "lib.typ", "regoli.wasm", "README.md",
    "LICENSE", "THIRD-PARTY-LICENSES.txt",
]


def main():
    manifest = tomllib.loads((ROOT / "typst.toml").read_text(encoding="utf-8"))["package"]
    assert manifest["name"] == "regoli"
    assert manifest["version"] == "0.1.0"
    assert manifest["entrypoint"] == "lib.typ"
    for field in ["authors", "description", "license"]:
        assert manifest[field], f"Missing publishing metadata: {field}"
    output = ROOT / "tests" / "output"
    packages = output / "packages"
    package = packages / "local" / manifest["name"] / manifest["version"]
    package.mkdir(parents=True, exist_ok=True)
    for name in RUNTIME:
        shutil.copyfile(ROOT / name, package / name)
    subprocess.run(
        ["typst", "compile", "--root", str(ROOT), "--package-path", str(packages),
         str(ROOT / "tests" / "package.typ"), str(output / "package.pdf")],
        cwd=ROOT, check=True,
    )
    print("Verified manifest and isolated @local package import.")


if __name__ == "__main__":
    main()
