$ErrorActionPreference = "Stop"
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    cargo build --release --locked --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { throw "WASM build failed" }
    Copy-Item "target\wasm32-unknown-unknown\release\regoli.wasm" "regoli.wasm"
} finally {
    Pop-Location
}
