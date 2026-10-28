$ErrorActionPreference = 'Stop'
$appRoot = Split-Path -Parent $PSScriptRoot
$existingTools = Join-Path (Split-Path -Parent $appRoot) 'local-flow\.tools'
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) { $cargoPath = $cargo.Source }
elseif (Test-Path (Join-Path $existingTools 'cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $existingTools 'cargo'
    $env:RUSTUP_HOME = Join-Path $existingTools 'rustup'
    $cargoPath = Join-Path $existingTools 'cargo\bin\cargo.exe'
} else { throw 'Install a Rust toolchain before building Dropflip.' }
$compiler = Join-Path $existingTools 'compiler\w64devkit\bin'
if (Test-Path $compiler) { $env:PATH = $compiler + ';' + $env:PATH }
Push-Location $appRoot
try { & $cargoPath @args; if ($LASTEXITCODE -ne 0) { throw "Cargo exited with $LASTEXITCODE" } }
finally { Pop-Location }
