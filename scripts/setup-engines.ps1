$ErrorActionPreference = 'Stop'
$appRoot = Split-Path -Parent $PSScriptRoot
& node (Join-Path $PSScriptRoot 'fetch-engines.mjs')
if ($LASTEXITCODE -ne 0) { throw 'Engine download or verification failed' }
$tools = Join-Path $appRoot 'tools'
$cache = Join-Path $appRoot '.tools/downloads'
foreach ($engine in @('pandoc','typst')) {
    $archive = Join-Path $cache "$engine.zip"
    $unpacked = Join-Path $cache "$engine-unpacked"
    Expand-Archive -LiteralPath $archive -DestinationPath $unpacked -Force
    $binary = Get-ChildItem -LiteralPath $unpacked -Filter "$engine.exe" -Recurse | Select-Object -First 1
    if (-not $binary) { throw "Missing $engine executable in archive" }
    Copy-Item -LiteralPath $binary.FullName -Destination (Join-Path $tools "$engine.exe") -Force
}
Write-Output 'Portable document engines are ready in the app tools folder.'
