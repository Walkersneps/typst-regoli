#import "@local/regoli:0.1.0": decode-npz

#let arrays = decode-npz(read("../../examples/sample.npz", encoding: none))
#assert.eq(arrays.matrix, ((0.0, 1.0, 2.0), (3.0, 4.0, 5.0)))
