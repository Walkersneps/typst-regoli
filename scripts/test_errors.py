"""Verify that invalid NPZ inputs produce useful Typst diagnostics."""

from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "tests" / "output"


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    cases = {
        'read("../fixtures/object.npz", encoding: none)': "object arrays",
        'read("../fixtures/duplicate.npz", encoding: none)': "duplicate array name",
        'read("../fixtures/bad-crc.npz", encoding: none)': "checksum",
        'read("../fixtures/uint64-overflow.npz", encoding: none)': "exact integer range",
        'read("../fixtures/strings.npz", encoding: none)': "unsupported dtype",
        'bytes("invalid zip")': "regoli",
        '"file.npz"': "decode-npz expects bytes",
        'bytes(""), metadata: "yes"': "metadata must be a boolean",
    }
    for index, (expression, expected) in enumerate(cases.items()):
        source = OUTPUT / f"error-{index}.typ"
        source.write_text(
            '#import "../../src/lib.typ": decode-npz\n'
            f"#decode-npz({expression})\n",
            encoding="utf-8", newline="\n",
        )
        if shutil.which("typstyle"):
            check = subprocess.run(["typstyle", "--check", str(source)], capture_output=True)
            if check.returncode:
                diff = subprocess.run(["typstyle", "--diff", str(source)])
                if diff.returncode not in (0, 1):
                    diff.check_returncode()
                subprocess.run(["typstyle", "-i", str(source)], check=True)
        result = subprocess.run(
            ["typst", "compile", "--root", str(ROOT), str(source),
             str(OUTPUT / f"error-{index}.pdf")],
            capture_output=True, text=True, encoding="utf-8",
        )
        if result.returncode == 0 or expected not in result.stderr:
            raise AssertionError(f"{expression}: expected {expected!r}\n{result.stderr}")
    print(f"Verified {len(cases)} Typst error diagnostics.")


if __name__ == "__main__":
    main()
