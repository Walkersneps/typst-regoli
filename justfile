# Local Variables:
# mode: makefile
# End:
bundle-file := ".dist/regoli.tar.gz"

build:
  cd regoli-core; \
  cargo build --release \
    --target wasm32-unknown-unknown; \
  cp target/wasm32-unknown-unknown/release/regoli.wasm regoli.wasm

bundle: build
  tpix bundle . -o "{{bundle-file}}"

publish: bundle
  tpix push "{{bundle-file}}" sneps