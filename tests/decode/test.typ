#import "/src/lib.typ": decode-npz, npz

#let arrays = decode-npz(
  read("../fixtures/compressed.npz", encoding: none),
  metadata: true,
)

#let cases = (
  ("matrix", (2, 3), "<f8", ((0.0, 1.0, 2.0), (3.0, 4.0, 5.0))),
  (
    "fortran",
    (2, 3, 4),
    ">i4",
    (
      ((0, 1, 2, 3), (4, 5, 6, 7), (8, 9, 10, 11)),
      ((12, 13, 14, 15), (16, 17, 18, 19), (20, 21, 22, 23)),
    ),
  ),
  ("scalar", (), ">i8", 42),
  ("empty", (2, 0, 3), "<f4", ((), ())),
  ("empty-leading", (0, 3), "<f8", ()),
  ("bools", (2, 2), "|b1", ((true, false), (false, true))),
  (
    "complex",
    (2,),
    ">c16",
    ((real: 1.0, imag: 2.0), (real: -3.0, imag: -4.0)),
  ),
  ("int64", (2,), "<i8", (-9223372036854775807 - 1, 9223372036854775807)),
  ("uint64", (2,), "<u8", (0, 9223372036854775807)),
  ("nested/name", (1,), "<i8", (7,)),
  ("fortran-empty", (2, 0, 3), "<i2", ((), ())),
  ("scalar-complex", (), "<c8", (real: 1.5, imag: -2.25)),
  ("scalar-bool", (), "|b1", true),
)

#{
  for (name, shape, dtype, data) in cases {
    let array = arrays.at(name)
    assert.eq(array.shape, shape)
    assert.eq(array.dtype, dtype)
    assert.eq(array.data, data)
  }

  for (kind, widths) in (
    ("i", (1, 2, 4, 8)),
    ("u", (1, 2, 4, 8)),
    ("f", (2, 4, 8)),
    ("c", (8, 16)),
  ) {
    for width in widths {
      for (endian, label) in (("<", "le"), (">", "be")) {
        let suffix = str(width)
        let name = label + "-" + kind + suffix
        let prefix = if width == 1 and (kind == "i" or kind == "u") {
          "|"
        } else {
          endian
        }
        let expected = if kind == "u" {
          (0, 1, 12)
        } else if kind == "f" {
          (-2.0, 0.0, 3.0)
        } else if kind == "c" {
          (
            (real: -2.0, imag: 0.0),
            (real: 0.0, imag: 0.0),
            (real: 3.0, imag: 0.0),
          )
        } else {
          (-2, 0, 3)
        }
        let array = arrays.at(name)
        assert.eq(array.shape, (3,))
        assert.eq(array.dtype, prefix + kind + suffix)
        assert.eq(array.data, expected)
      }
    }
  }

  for (name, dtype, data) in (
    (
      "float-edges-2",
      "<f2",
      (-0.0, 0.5, -1.25, 65504.0, 6.103515625e-5, 5.960464477539063e-8),
    ),
    (
      "float-edges-4",
      "<f4",
      (
        -0.0,
        0.5,
        -1.25,
        3.4028234663852886e38,
        1.1754943508222875e-38,
        1.401298464324817e-45,
      ),
    ),
    (
      "float-edges-8",
      "<f8",
      (
        -0.0,
        0.5,
        -1.25,
        1.7976931348623157e308,
        2.2250738585072014e-308,
        5e-324,
      ),
    ),
  ) {
    let array = arrays.at(name)
    assert.eq(array.dtype, dtype)
    assert.eq(array.data, data)
  }

  assert.eq(arrays.at("fortran").at("fortran-order"), true)
  assert.eq(arrays.at("matrix").at("fortran-order"), false)
  assert.eq(arrays.at("special").data.at(0), calc.inf)
  assert.eq(arrays.at("special").data.at(1), -calc.inf)
  assert(arrays.at("special").data.at(2).is-nan())
  assert.eq(arrays.at("special").data.at(3).to-bytes().at(7), 128)
  assert.eq(type(arrays.at("matrix").data.at(0).at(0)), float)
  assert.eq(type(arrays.at("int64").data.at(0)), int)
  assert.eq(type(arrays.at("bools").data.at(0).at(0)), bool)

  let source = read("../fixtures/compressed.npz", encoding: none)
  assert.eq(
    decode-npz(source, every: 2).matrix,
    ((0.0, 1.0, 2.0),),
  )
  assert.eq(
    decode-npz(source, start: 1, max-rows: 1).matrix,
    ((3.0, 4.0, 5.0),),
  )
  assert.eq(
    decode-npz(source, every: 2, start: 1, max-rows: 1).matrix,
    ((3.0, 4.0, 5.0),),
  )
  assert.eq(npz(source, every: 2, start: 1).matrix, ((3.0, 4.0, 5.0),))
  assert.eq(
    decode-npz(source, every: 2, start: 1, metadata: true).matrix.shape,
    (1, 3),
  )
  assert.eq(
    decode-npz(source, every: 2, start: 1, metadata: true).matrix.data,
    ((3.0, 4.0, 5.0),),
  )
  let sampled = decode-npz(source, every: 2, start: 1, metadata: true)
  assert.eq(sampled.fortran.shape, (1, 3, 4))
  assert.eq(
    sampled.fortran.data,
    (((12, 13, 14, 15), (16, 17, 18, 19), (20, 21, 22, 23)),),
  )
  assert.eq(sampled.scalar.data, 42)
  assert.eq(decode-npz(source, max-rows: 0).matrix, ())

  let stored = decode-npz(read("../fixtures/stored.npz", encoding: none))
  assert.eq(stored.matrix, ((0.0, 1.0, 2.0), (3.0, 4.0, 5.0)))

  let positional = decode-npz(read(
    "../fixtures/positional.npz",
    encoding: none,
  ))
  assert.eq(positional.arr_0, (1, 2))
  assert.eq(positional.arr_1, 3.5)

  let versions = decode-npz(read("../fixtures/versions.npz", encoding: none))
  assert.eq(versions.v2, (2.5,))
  assert.eq(versions.v3, (3.5,))

  assert.eq(decode-npz(read("../fixtures/empty.npz", encoding: none)), (:))
  assert.eq(
    decode-npz(read("../fixtures/directories.npz", encoding: none)).at(
      "folder/x",
    ),
    (0, 1, 2, 3),
  )
}
