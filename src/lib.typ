#let decoder = plugin("../regoli.wasm")

/// Decode a NumPy NPZ byte buffer into a dictionary keyed by array name.
///
/// `source` must be bytes, usually from `read(path, encoding: none)`.
/// `metadata` controls whether each value is the metadata dictionary or just
/// its nested `data`. `every`, `start`, and `max-rows` select rows on the first
/// axis of each non-scalar array; all archive data is still validated.
/// Metadata dictionaries contain `data`, `shape`, `dtype`, and `fortran-order`.
#let decode-npz(
  source,
  metadata: false,
  every: 1,
  start: 0,
  max-rows: none,
) = {
  assert(
    type(source) == bytes,
    message: "decode-npz expects bytes; use read(path, encoding: none)",
  )
  assert(type(metadata) == bool, message: "metadata must be a boolean")
  assert(
    type(every) == int and every > 0,
    message: "every must be a positive integer",
  )
  assert(
    type(start) == int and start >= 0,
    message: "start must be a non-negative integer",
  )
  assert(
    max-rows == none or (type(max-rows) == int and max-rows >= 0),
    message: "max-rows must be a non-negative integer or none",
  )
  // CBOR carries selection options into the decoder, where row filtering can
  // happen before nested Typst values are allocated.
  let options = cbor.encode((every: every, start: start, "max-rows": max-rows))
  let arrays = cbor(decoder.decode_with_options(source, options))
  // Preserve archive names while hiding metadata unless the caller requests it.
  arrays
    .pairs()
    .map(((name, value)) => (name, if metadata { value } else { value.data }))
    .to-dict()
}

/// Load an NPZ archive from a path or bytes and decode it.
///
/// All options are forwarded unchanged to `decode-npz`.
#let npz(
  source,
  metadata: false,
  every: 1,
  start: 0,
  max-rows: none,
) = {
  let data = if type(source) == path {
    read(source, encoding: none)
  } else if type(source) == bytes {
    source
  } else {
    assert(false, message: "npz expects either a path or bytes")
  }
  decode-npz(
    data,
    metadata: metadata,
    every: every,
    start: start,
    max-rows: max-rows,
  )
}
