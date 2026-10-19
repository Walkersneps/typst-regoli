#let decoder = plugin("../regoli.wasm")

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

#let npz(source, metadata: false) = {
  let data = if type(source) == path {
    read(source, encoding: none)
  } else if type(source) == bytes {
    source
  } else {
    assert(false, message: "npz expects either a path or bytes")
  }
  decode-npz(data, metadata: metadata)
}
