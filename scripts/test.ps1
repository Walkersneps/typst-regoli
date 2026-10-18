$ErrorActionPreference = "Stop"
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    python "scripts\generate_fixtures.py"
    if ($LASTEXITCODE -ne 0) { throw "Fixture generation failed" }
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw "Rust tests failed" }
    cargo clippy --locked --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "Rust lint failed" }
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw "Rust format check failed" }
    & "scripts\build.ps1"
    if (Get-Command typstyle -ErrorAction SilentlyContinue) {
        typstyle --check "lib.typ" "examples\example.typ" "tests\package.typ"
        if ($LASTEXITCODE -ne 0) { throw "Typst format check failed" }
        typstyle --check "tests\fixtures\integration.typ"
        if ($LASTEXITCODE -ne 0) {
            typstyle --diff "tests\fixtures\integration.typ"
            typstyle -i "tests\fixtures\integration.typ"
            if ($LASTEXITCODE -ne 0) { throw "Generated assertion formatting failed" }
        }
    }
    New-Item -ItemType Directory -Force "tests\output" | Out-Null
    typst compile --root . "tests\fixtures\integration.typ" "tests\output\integration.pdf"
    if ($LASTEXITCODE -ne 0) { throw "Typst numerical assertions failed" }
    typst compile --root . "examples\example.typ" "tests\output\example.pdf"
    if ($LASTEXITCODE -ne 0) { throw "Typst example failed" }
    python "scripts\test_errors.py"
    if ($LASTEXITCODE -ne 0) { throw "Typst error propagation tests failed" }
    python "scripts\test_package.py"
    if ($LASTEXITCODE -ne 0) { throw "Isolated package import failed" }
} finally {
    Pop-Location
}
