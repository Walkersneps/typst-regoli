#let decoder = plugin("regoli.wasm")

/// Decode an NPZ byte buffer into a dictionary of arrays.
/// With metadata enabled, each value has data, shape, dtype, and fortran-order fields.
#let decode-npz(source, metadata: false) = {
  assert(
    type(source) == bytes,
    message: "decode-npz expects bytes; use read(path, encoding: none)",
  )
  assert(type(metadata) == bool, message: "metadata must be a boolean")
  let arrays = cbor(decoder.decode(source))
  if metadata {
    arrays
  } else {
    arrays.pairs().map(((name, value)) => (name, value.data)).to-dict()
  }
}
