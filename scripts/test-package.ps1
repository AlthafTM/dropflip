param([Parameter(Mandatory)][string]$ReleaseDirectory)
$ErrorActionPreference = 'Stop'
$release = [IO.Path]::GetFullPath($ReleaseDirectory)
$archives = @(Get-ChildItem -LiteralPath $release -File -Filter '*.zip')
$installers = @(Get-ChildItem -LiteralPath $release -File -Filter '*-setup.exe')
if ($archives.Count -ne 1 -or $installers.Count -ne 1) { throw 'Expected one portable ZIP and one installer' }
$appKey = 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/{986B87CA-AC4D-4E6C-AB43-D338815A5129}_is1'
if (Test-Path $appKey) { throw 'Use a clean Windows account for installer QA; Dropflip is already installed' }
$work = Join-Path ([IO.Path]::GetTempPath()) ('dropflip-package-qa-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$source = Join-Path $work 'sample.txt'
[IO.File]::WriteAllText($source, 'Dropflip package smoke fixture')
$startup = Join-Path ([Environment]::GetFolderPath('Startup')) 'Dropflip.lnk'
$startupHash = if (Test-Path -LiteralPath $startup) { (Get-FileHash -LiteralPath $startup).Hash } else { $null }
Expand-Archive -LiteralPath $archives[0].FullName -DestinationPath (Join-Path $work 'portable')
$executables = @(Get-ChildItem -LiteralPath (Join-Path $work 'portable') -Recurse -Filter Dropflip.exe)
if ($executables.Count -ne 1) { throw 'Expected one portable application executable' }
$portable = $executables[0].FullName

function Test-NativeConversion([string]$Executable, [string]$Destination) {
    $process = Start-Process -FilePath $Executable -ArgumentList @('--convert', "`"$source`"", 'zip', "`"$Destination`"") -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw 'Native package conversion failed' }
    $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $Destination 'sample.zip'))
    try {
        if ($zip.Entries.Count -ne 1 -or $zip.Entries[0].Name -ne 'sample.txt') { throw 'Unexpected archive content' }
        $reader = [IO.StreamReader]::new($zip.Entries[0].Open())
        try { if ($reader.ReadToEnd() -ne [IO.File]::ReadAllText($source)) { throw 'Converted bytes differ' } }
        finally { $reader.Dispose() }
    } finally { $zip.Dispose() }
}
Test-NativeConversion $portable (Join-Path $work 'portable-output')
$installed = Join-Path $work 'installed'
$process = Start-Process -FilePath $installers[0].FullName -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/NOICONS', '/TASKS=""', "/DIR=`"$installed`"", "/LOG=`"$work/install.log`"") -WindowStyle Hidden -PassThru -Wait
if ($process.ExitCode -ne 0) { throw 'Silent installer failed' }
$installedExe = Join-Path $installed 'Dropflip.exe'
if ((Get-FileHash -LiteralPath $portable).Hash -ne (Get-FileHash -LiteralPath $installedExe).Hash) { throw 'Installer and portable binaries differ' }
Test-NativeConversion $installedExe (Join-Path $work 'installed-output')
$keep = Join-Path $installed 'keep-user-file.txt'
Set-Content -LiteralPath $keep -Value 'Preserve this user-created file'
$uninstallers = @(Get-ChildItem -LiteralPath $installed -Filter 'unins*.exe')
if ($uninstallers.Count -ne 1) { throw 'Uninstaller missing' }
$process = Start-Process -FilePath $uninstallers[0].FullName -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$work/uninstall.log`"") -WindowStyle Hidden -PassThru -Wait
if ($process.ExitCode -ne 0 -or (Test-Path -LiteralPath $installedExe)) { throw 'Uninstall failed' }
if (-not (Test-Path -LiteralPath $keep) -or (Get-Content -LiteralPath $keep -Raw).Trim() -ne 'Preserve this user-created file') { throw 'Uninstall removed unrelated user data' }
$afterStartupHash = if (Test-Path -LiteralPath $startup) { (Get-FileHash -LiteralPath $startup).Hash } else { $null }
if ($startupHash -ne $afterStartupHash) { throw 'Installer QA changed the user startup shortcut' }
if (Test-Path $appKey) { throw 'Uninstall registration remains' }
Write-Output "Portable conversion, installer conversion, matching binaries, uninstall and user-file preservation passed: $work"
