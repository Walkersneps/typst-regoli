#import "../src/lib.typ": decode-npz

#let arrays = decode-npz(read("sample.npz", encoding: none))
#assert.eq(arrays.matrix, ((0.0, 1.0, 2.0), (3.0, 4.0, 5.0)))

= NumPy data in Typst

#table(
  columns: 3,
  ..arrays.matrix.flatten().map(value => [#value]),
)
