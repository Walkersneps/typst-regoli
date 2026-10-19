# Local Variables:
# mode: makefile
# End:
bundle-file := ".dist/regoli.tar.gz"
test-packages := "tests/output/packages"
test-package := "tests/output/packages/local/regoli/0.1.0"

build:
  cd regoli-core; \
  cargo build --release \
    --target wasm32-unknown-unknown; \
  cp target/wasm32-unknown-unknown/release/regoli.wasm ../regoli.wasm

bundle: build
  tpix bundle . -o "{{bundle-file}}"

publish: bundle
  tpix push "{{bundle-file}}" sneps

test: lint testonly

testonly: test-core test-library

prepare-test: build
  python "scripts/generate_fixtures.py"
  mkdir -p "{{test-package}}/src"
  cp "typst.toml" "{{test-package}}/typst.toml"
  cp "src/lib.typ" "{{test-package}}/src/lib.typ"
  cp "regoli.wasm" "{{test-package}}/regoli.wasm"
  cp "README.md" "{{test-package}}/README.md"
  cp "LICENSE" "{{test-package}}/LICENSE"
  cp "regoli-core/THIRD-PARTY-LICENSES.txt" "{{test-package}}/THIRD-PARTY-LICENSES.txt"

lint-core:
  cargo clippy --locked --all-targets --manifest-path "regoli-core/Cargo.toml" -- -D warnings
  cargo fmt --check --manifest-path "regoli-core/Cargo.toml"

lint-library:
  typstyle --check "src/lib.typ" "examples/example.typ" "tests/decode/test.typ" "tests/errors/test.typ" "tests/package-import/test.typ" "tests/example/test.typ"

lint: lint-core lint-library

test-core:
  cargo test --locked --manifest-path "regoli-core/Cargo.toml"

test-library *filter: prepare-test
  tt run --no-fail-fast --package-path "{{test-packages}}" {{filter}}
