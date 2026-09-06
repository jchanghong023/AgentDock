# PowerShell 7. Formats repository sources; does not modify application state.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'BLOCKED: cargo not installed; no Rust verification was performed.'
}
function Invoke-Cargo {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CargoArgs)
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo failed with exit code $LASTEXITCODE" }
}
Invoke-Cargo --version
& rustc --version
if ($LASTEXITCODE -ne 0) { throw 'rustc failed' }
Invoke-Cargo fmt --all
if (-not (Test-Path 'Cargo.lock')) { Invoke-Cargo generate-lockfile }
Invoke-Cargo check --locked --all-targets
Invoke-Cargo test --locked --all-targets
Invoke-Cargo clippy --locked --all-targets
Invoke-Cargo run --locked -- --self-test
Write-Output 'Rust build/check/test/PTY checks completed on Windows. CentOS 7 remains unverified.'
