#import "/src/lib.typ": decode-npz

#let expect-diagnostic(action, fragment) = {
  let message = catch(action)
  assert(message != none, message: "Expected the operation to fail.")
  assert(
    str.contains(message, fragment),
    message: "Expected diagnostic to contain: " + fragment,
  )
}

#expect-diagnostic(
  () => decode-npz(read("../fixtures/object.npz", encoding: none)),
  "object arrays",
)
#expect-diagnostic(
  () => decode-npz(read("../fixtures/duplicate.npz", encoding: none)),
  "duplicate array name",
)
#expect-diagnostic(
  () => decode-npz(read("../fixtures/bad-crc.npz", encoding: none)),
  "checksum",
)
#expect-diagnostic(
  () => decode-npz(read("../fixtures/uint64-overflow.npz", encoding: none)),
  "exact integer range",
)
#expect-diagnostic(
  () => decode-npz(read("../fixtures/strings.npz", encoding: none)),
  "unsupported dtype",
)
#expect-diagnostic(() => decode-npz(bytes("invalid zip")), "regoli")
#expect-diagnostic(() => decode-npz("file.npz"), "decode-npz expects bytes")
#expect-diagnostic(
  () => decode-npz(bytes(""), metadata: "yes"),
  "metadata must be a boolean",
)
