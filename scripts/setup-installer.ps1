param([string]$Destination = (Join-Path $PSScriptRoot '../.tools/inno'))
$ErrorActionPreference = 'Stop'
$destinationPath = [IO.Path]::GetFullPath($Destination)
$compiler = Join-Path $destinationPath 'ISCC.exe'
if (Test-Path -LiteralPath $compiler) { Write-Output $compiler; exit 0 }
New-Item -ItemType Directory -Force -Path $destinationPath | Out-Null
$download = Join-Path $destinationPath 'innosetup-6.7.3.exe'
Invoke-WebRequest 'https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe' -OutFile $download
$expected = '9c73c3bae7ed48d44112a0f48e66742c00090bdb5bef71d9d3c056c66e97b732'
if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ne $expected) { throw 'Inno Setup checksum mismatch' }
$process = Start-Process -FilePath $download -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/NOICONS', '/TASKS=""', "/DIR=`"$destinationPath`"") -WindowStyle Hidden -PassThru -Wait
if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $compiler)) { throw 'Inno Setup installation failed' }
Write-Output $compiler
