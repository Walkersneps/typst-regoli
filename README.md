# regoli

Import NumPy `.npz` archives into Typst as numerical arrays.

Both
`numpy.savez` (stored ZIP) and
`numpy.savez_compressed` (DEFLATE ZIP) work.
The bundled WebAssembly decoder runs inside Typst: users do not need
Python, NumPy, Rust, filesystem plugins, or preprocessing.

Requires **Typst 0.15.0 or newer**.
This package is not yet published on
Typst Universe.
It is currently published on the alternative package registry [TPIX](https://tpix.typstify.com).

## Quick start

Generate an archive in Python:

```python
import numpy as np

np.savez_compressed(
    "results.npz",
    matrix=np.array([[1.0, 2.0], [3.0, 4.0]]),
    counts=np.array([10, 20], dtype=np.int32),
)
```

In your typst document, import the library:

```typst
#import "@sneps/regoli:0.1.0" as regoli

#let data = regoli.npz(path("results.npz"))
#assert.eq(results.matrix, ((1.0, 2.0), (3.0, 4.0)))
#assert.eq(results.counts, (10, 20))

#table(columns: 2, ..results.matrix.flatten().map(x => [#x]))
```

Then, before compiling the document, have TPIX fetch your project's dependencies (including ```regoli```):

```bash
tpix pull
```

This will download the `regoli` package and any other dependencies specified in Typst's local cache,
making them available for your project.

You can now compile your document as usual.

## API

### `decode-npz(source, metadata: false)`

- `source`: raw `bytes`, normally obtained with `read(path, encoding: none)`.
- `metadata`: boolean, default `false`.
- Returns a dictionary keyed by archive member name, removing only the final
  `.npy` suffix. Names such as `nested/name` are preserved; use `.at(name)`
  when a name is not a Typst identifier. Positional `numpy.savez` arrays
  become `arr_0`, `arr_1`, and so on.

By default, values are nested Typst arrays, indexed in logical NumPy axis
order regardless of C/Fortran storage order. A zero-dimensional NumPy array
becomes a scalar; an empty dimension becomes an empty Typst array.

Enable metadata to retain exact shapes, including dimensions after a zero:

```typst
#import "lib.typ": decode-npz

#let results = decode-npz(read("results.npz", encoding: none), metadata: true)
#let matrix = results.matrix
#assert.eq(matrix.shape, (2, 2))
#assert.eq(matrix.data, ((1.0, 2.0), (3.0, 4.0)))
// matrix.dtype is the NPY descriptor, for example "<f8".
// matrix.fortran-order records the file's storage order.
```

Each metadata value is a dictionary with `data`, `shape`, `dtype`, and
`fortran-order`. `shape` is an array of dimensions; scalars have shape `()`.
An archive with no arrays returns `(:)`.

### `npz(source, metadata: false)`

Wrapper around `decode-npz` that accepts either a file path or raw bytes.

## Supported data and limitations

| NumPy dtype | Typst value |
| --- | --- |
| `bool` | `bool` |
| `int8`, `int16`, `int32`, `int64` | Exact `int` |
| `uint8`, `uint16`, `uint32`, `uint64` | Exact `int`, subject to the range below |
| `float16`, `float32`, `float64` | `float` (64-bit) |
| `complex64`, `complex128` | Dictionary `(real: float, imag: float)` per element |

- NPY versions 1.0, 2.0, and 3.0, explicit little/big endian descriptors,
  and C/Fortran ordering are supported. Byte-order-independent descriptors
  (`|`) are supported for single-byte types. Native-endian (`=`) descriptors
  are rejected because the writer's byte order cannot be inferred portably;
  standard NumPy writers emit explicit byte order.
- NaN, positive/negative infinity, and signed zero are preserved through CBOR.
  Floating-point conversion retains numerical values, not NaN payload bits.
- `uint64` values above `9223372036854775807` fail explicitly rather than
  silently losing precision, since Typst integers are signed 64-bit.
- Object/pickled arrays are **never executed or deserialized**. String,
  structured/record, void, datetime, timedelta, extended-precision float,
  and other unsupported dtypes produce errors. The NPY header reader accepts
  NumPy's standard unescaped ASCII keys and primitive dtype strings, not
  arbitrary Python expressions or escaped spellings.
- Only stored and DEFLATE compression are enabled. Encrypted archives and
  unsupported compression methods fail. Directory entries are ignored;
  other non-NPY entries, duplicate array names, corrupt checksums, malformed
  headers, and mismatched data lengths fail with Typst diagnostics.
- Limits per call: 64 MiB archive input, 64 MiB total decompressed member
  data, 1,024 ZIP entries, 64 KiB per NPY header, rank 32, and 1,000,000
  logical array nodes (containers plus elements, summed across arrays).
  Limits are fixed to bound memory use; this is intended for document-sized
  datasets, not large-scale numerical processing. Typst compilation memory
  can be substantially larger than the source archive.

## Build and test

The ready-to-use `regoli.wasm` is included. To rebuild it, install a
Rust toolchain and the WebAssembly target:

```powershell
rustup target add wasm32-unknown-unknown
```

Then, you can start a build with [just](https://github.com/casey/just):

```powershell
just build
```

The build targets a pure
`wasm32-unknown-unknown` module with no WASI, filesystem, network, or Python
imports, implementing Typst's minimal plugin protocol.

Development tests additionally require Python with NumPy, Typst, and
optionally `typstyle`:

```powershell
python -m pip install -r requirements-dev.txt
.\scripts\test.ps1
```

The test script generates real NumPy fixtures, runs Rust unit tests and
Clippy, builds the WASM, checks/formats generated Typst assertions, compiles
the example and integration assertions, checks failing Typst diagnostics,
and verifies an isolated `@local` package import using runtime files only.
Fixtures cover both ZIP compression modes, NPY header versions, all supported
dtype widths and endiannesses, scalars, empty dimensions, Fortran ordering,
integer bounds, special floats, and invalid input. Test outputs stay under
`tests/output`; generated fixtures are not part of the published package.

After changing Rust dependencies, regenerate bundled license notices:

```powershell
python scripts\generate_licenses.py
```

## Local package installation

Following the [Typst package repository guidelines](https://github.com/typst/packages),
you can test the package locally during development or before publishing by
copying the project runtime files into Typst's local package directory, e.g.,
`%APPDATA%\typst\packages\local\regoli\{version-num}\` on Windows.
For more information, refer to the [relevant documentation section](https://github.com/typst/packages#local-packages).

Then import with:

```typst
#import "@local/regoli:0.1.0": decode-npz
#let results = decode-npz(read("results.npz", encoding: none))
```

Use `typst info` to locate your actual package data directory when supported
by your compiler.

## Publishing to a package registry

The root `typst.toml` declares the package entrypoint, version, compiler
requirement, author, license, and exclusions for build/test-only files.
The distinctive package name follows the repository's naming guidelines.
Name availability and acceptance still need to be confirmed at submission.

1. Rebuild and run all tests; regenerate dependency license notices.
2. Bump the project version
3. Publishing paths differ depending on the target registry:

### TPIX

You can directly bundle and publish the package with the corresponding *just* recipe.

```powershell
just publish
```

### Typst Universe

1. Fork/clone [typst/packages](https://github.com/typst/packages) and add the
   package under `packages/preview/regoli/0.1.0`.
2. Include the runtime files (those not explicitly excluded in `typst.toml`) and,
   optionally, `examples/` with its
   `sample.npz`. Keep build outputs, test fixtures, and PDFs out of the bundle.
   Keep Rust sources and build scripts in your development repository so
   reviewers can reproduce the binary.
3. Test an absolute package import using a temporary `preview` package path,
   then open a pull request with the binary's source/build instructions.
4. Only after acceptance is `@preview/regoli:{version}` publicly available.

## License

Original source: AGPL-3.0-only, copyright 2026 carliwal; see [LICENSE](LICENSE).
The bundled WASM includes third-party dependencies under their respective
licenses, including Apache-2.0. Their notices are preserved in
[THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt); retain that file when
redistributing the binary.
