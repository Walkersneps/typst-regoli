"""Create real NumPy archives and Typst assertions without a CBOR dependency."""

import io
import math
from pathlib import Path
import struct
import warnings
import zipfile

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures"


def npy(array, version=None):
    buffer = io.BytesIO()
    np.lib.format.write_array(buffer, array, version=version)
    return buffer.getvalue()


def archive(name, entries, compression=zipfile.ZIP_STORED):
    with zipfile.ZipFile(FIXTURES / name, "w", compression=compression) as output:
        for key, value in entries:
            output.writestr(key, value)


def typst(value):
    if isinstance(value, list):
        return "(" + "".join(typst(item) + "," for item in value) + ")"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, complex):
        return f"(real: {typst(value.real)}, imag: {typst(value.imag)})"
    if isinstance(value, float):
        if math.isnan(value):
            return "float.nan"
        if math.isinf(value):
            return ("-" if value < 0 else "") + "calc.inf"
        return repr(value)
    if value == -(2**63):
        return "(-9223372036854775807 - 1)"
    return str(value)


def main():
    FIXTURES.mkdir(parents=True, exist_ok=True)
    arrays = {
        "matrix": np.arange(6, dtype="<f8").reshape(2, 3),
        "fortran": np.asfortranarray(np.arange(24, dtype=">i4").reshape(2, 3, 4)),
        "scalar": np.array(42, dtype=">i8"),
        "empty": np.empty((2, 0, 3), dtype="f4"),
        "empty-leading": np.empty((0, 3), dtype="f8"),
        "bools": np.array([[True, False], [False, True]]),
        "complex": np.array([1 + 2j, -3 - 4j], dtype=">c16"),
        "special": np.array([np.inf, -np.inf, np.nan, -0.0]),
        "int64": np.array([-(2**63), 2**63 - 1], dtype="i8"),
        "uint64": np.array([0, 2**63 - 1], dtype="u8"),
        "nested/name": np.array([7]),
        "fortran-empty": np.asfortranarray(np.empty((2, 0, 3), dtype="i2")),
        "scalar-complex": np.array(1.5 - 2.25j, dtype="<c8"),
        "scalar-bool": np.array(True),
    }
    for kind, widths in [("i", [1, 2, 4, 8]), ("u", [1, 2, 4, 8]),
                         ("f", [2, 4, 8]), ("c", [8, 16])]:
        for width in widths:
            for endian, label in [("<", "le"), (">", "be")]:
                dtype = f"{endian}{kind}{width}"
                values = [0, 1, 12] if kind == "u" else [-2, 0, 3]
                arrays[f"{label}-{kind}{width}"] = np.array(values, dtype=dtype)
    for width in [2, 4, 8]:
        dtype = np.dtype(f"f{width}")
        info = np.finfo(dtype)
        arrays[f"float-edges-{width}"] = np.array(
            [-0.0, 0.5, -1.25, info.max, info.tiny, info.smallest_subnormal],
            dtype=dtype,
        )
    np.savez(FIXTURES / "stored.npz", **arrays)
    np.savez_compressed(FIXTURES / "compressed.npz", **arrays)
    np.savez(FIXTURES / "positional.npz", np.array([1, 2]), np.array(3.5))
    np.savez(FIXTURES / "object.npz", data=np.array([{}], dtype=object))
    np.savez(FIXTURES / "strings.npz", data=np.array(["hello"]))
    np.savez(FIXTURES / "structured.npz", data=np.array([(1,)], dtype=[("x", "i4")]))
    np.savez(FIXTURES / "uint64-overflow.npz", data=np.array([2**63], dtype="u8"))
    valid = npy(np.arange(4, dtype="i4"))
    archive("truncated.npz", [("data.npy", valid[:-1])])
    archive("trailing.npz", [("data.npy", valid + b"\0")])
    archive("bad-npy.npz", [("data.npy", b"not an NPY file")])
    archive("non-npy.npz", [("readme.txt", b"hello")])
    archive("empty.npz", [])
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", UserWarning)
        archive("duplicate.npz", [("data.npy", valid), ("data.npy", valid)])
    archive("bad-crc.npz", [("data.npy", valid)])
    corrupt = bytearray((FIXTURES / "bad-crc.npz").read_bytes())
    name_length, extra_length = struct.unpack_from("<HH", corrupt, 26)
    corrupt[30 + name_length + extra_length + len(valid) - 1] ^= 1
    (FIXTURES / "bad-crc.npz").write_bytes(corrupt)
    archive("too-many-nodes.npz", [("data.npy", npy(np.empty((1_000_001, 0))))])
    archive("versions.npz", [
        ("v2.npy", npy(np.array([2.5]), version=(2, 0))),
        ("v3.npy", npy(np.array([3.5]), version=(3, 0))),
    ])
    archive("directories.npz", [("folder/", b""), ("folder/x.npy", valid)])
    archive("too-many-entries.npz", [(f"{i}.npy", valid) for i in range(1025)])
    header_buffer = io.BytesIO()
    np.lib.format.write_array_header_1_0(
        header_buffer,
        {"descr": "<i8", "shape": (1,) * 33, "fortran_order": False},
    )
    archive("too-many-axes.npz", [("data.npy", header_buffer.getvalue() + b"\0" * 8)])
    assertions = ['#import "../../lib.typ": decode-npz',
                  '#let arrays = decode-npz(read("compressed.npz", encoding: none), metadata: true)']
    for name, array in arrays.items():
        key = f'arrays.at("{name}")'
        assertions.append(f'#assert.eq({key}.shape, {typst(list(array.shape))})')
        assertions.append(f'#assert.eq({key}.dtype, "{array.dtype.str}")')
        if name != "special":
            assertions.append(f'#assert.eq({key}.data, {typst(array.tolist())})')
    assertions += [
        '#assert.eq(arrays.fortran.fortran-order, true)',
        '#assert.eq(arrays.matrix.fortran-order, false)',
        '#assert.eq(arrays.special.data.at(0), calc.inf)',
        '#assert.eq(arrays.special.data.at(1), -calc.inf)',
        '#assert(arrays.special.data.at(2).is-nan())',
        '#assert.eq(arrays.special.data.at(3).to-bytes().at(7), 128)',
        '#assert.eq(type(arrays.matrix.data.at(0).at(0)), float)',
        '#assert.eq(type(arrays.int64.data.at(0)), int)',
        '#assert.eq(type(arrays.bools.data.at(0).at(0)), bool)',
        '#assert.eq(decode-npz(read("stored.npz", encoding: none)).matrix, ((0.0, 1.0, 2.0), (3.0, 4.0, 5.0)))',
        '#let positional = decode-npz(read("positional.npz", encoding: none))',
        '#assert.eq(positional.arr_0, (1, 2))',
        '#assert.eq(positional.arr_1, 3.5)',
        '#let versions = decode-npz(read("versions.npz", encoding: none))',
        '#assert.eq(versions.v2, (2.5,))',
        '#assert.eq(versions.v3, (3.5,))',
        '#assert.eq(decode-npz(read("empty.npz", encoding: none)), (:))',
        '#assert.eq(decode-npz(read("directories.npz", encoding: none)).at("folder/x"), (0, 1, 2, 3))',
        'NPZ integration assertions passed.',
    ]
    (FIXTURES / "integration.typ").write_text(
        "\n".join(assertions) + "\n", encoding="utf-8", newline="\n",
    )
    np.savez_compressed(ROOT / "examples" / "sample.npz",
                        matrix=np.arange(6, dtype="f8").reshape(2, 3))
    print(f"Generated {len(arrays)} numerical arrays and invalid-input fixtures.")


if __name__ == "__main__":
    main()
