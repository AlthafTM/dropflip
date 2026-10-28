param(
    [string]$BinaryPath = (Join-Path $PSScriptRoot '../target/release/dropflip.exe'),
    [string]$OutputRoot = (Join-Path $PSScriptRoot '../dist'),
    [string]$CompilerPath = (Join-Path $PSScriptRoot '../.tools/inno/ISCC.exe'),
    [string]$CargoCommand = 'cargo',
    [string]$Target = 'x86_64-pc-windows-msvc'
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+(?:-rc\.\d+)?$') { throw 'Unsupported release version' }
if (-not (Test-Path -LiteralPath $BinaryPath) -or -not (Test-Path -LiteralPath $CompilerPath)) { throw 'Build the app and run setup-installer.ps1 first' }
$binary = [IO.Path]::GetFullPath($BinaryPath)
if ([Diagnostics.FileVersionInfo]::GetVersionInfo($binary).ProductVersion -ne $version) { throw 'Executable version does not match Cargo.toml' }
$release = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) $version
$bundle = Join-Path $release "Dropflip-v$version-windows-x64"
if (Test-Path -LiteralPath $bundle) { throw 'Bundle directory already exists; choose a fresh OutputRoot' }
New-Item -ItemType Directory -Path $bundle -Force | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $bundle 'Dropflip.exe')
foreach ($file in @('LICENSE', 'README.md', 'SECURITY.md', 'THIRD_PARTY.md', 'CONVERSIONS.md')) {
    Copy-Item -LiteralPath (Join-Path $root $file) -Destination $bundle
}
Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination $bundle -Recurse
New-Item -ItemType Directory -Force -Path (Join-Path $bundle 'assets/screenshots'), (Join-Path $bundle 'qa') | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'assets/dropflip.png') -Destination (Join-Path $bundle 'assets/dropflip.png')
Copy-Item -LiteralPath (Join-Path $root 'assets/screenshots/conversion-panel.png') -Destination (Join-Path $bundle 'assets/screenshots/conversion-panel.png')
Copy-Item -LiteralPath (Join-Path $root 'qa/README.md') -Destination (Join-Path $bundle 'qa/README.md')
Copy-Item -LiteralPath (Join-Path $root 'assets/DICTATELY-LICENSE.txt') -Destination (Join-Path $bundle 'DICTATELY-LICENSE.txt')
Set-Content -LiteralPath (Join-Path $bundle 'START-HERE.txt') -Encoding utf8 -Value @'
Dropflip portable for Windows x64

Run Dropflip.exe. No administrator rights are required.
Keep this folder together. To update, exit Dropflip from its tray menu and extract
the new version into a new folder. Choose an output folder in the app if desired.

This compact package includes the native Rust app. External conversion engines
are not bundled. Native image/data/spreadsheet routes and a subset of PDF routes
work directly; PDF-to-Word/OCR, media and Office conversions require additional
engines. Read docs/DEVELOPMENT.md and THIRD_PARTY.md for setup and licenses.

Use the matching setup.exe instead if you prefer shortcuts and an uninstaller.
The current binaries are unsigned. Verify SHA256SUMS.txt from the same release.
'@
Push-Location $root
try {
    $metadataText = & $CargoCommand metadata --format-version 1 --locked --filter-platform $Target
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $metadata = ($metadataText | Out-String) | ConvertFrom-Json
    $revision = (& git rev-parse HEAD | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Git revision unavailable' }
} finally { Pop-Location }
$resolvedIds = @{}; foreach ($node in $metadata.resolve.nodes) { $resolvedIds[$node.id] = $true }
$notices = @()
$licenseCache = Join-Path $root '.tools/release-license-cache'
foreach ($package in $metadata.packages) {
    if (-not $package.source -or -not $resolvedIds.ContainsKey($package.id)) { continue }
    $packageRoot = Split-Path -Parent $package.manifest_path
    $licenseFiles = @(Get-ChildItem -LiteralPath $packageRoot -Recurse -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT|OFL|UFL)' })
    if ($package.license_file) { $licenseFiles += Get-Item -LiteralPath (Join-Path $packageRoot $package.license_file) }
    $licenseOrigin = 'crate archive'
    if ($licenseFiles.Count -eq 0) {
        $vcsFile = Join-Path $packageRoot '.cargo_vcs_info.json'
        if (-not (Test-Path -LiteralPath $vcsFile)) { throw "No license or source revision for $($package.name)" }
        $vcs = Get-Content -LiteralPath $vcsFile -Raw | ConvertFrom-Json
        $repository = [Uri]$package.repository
        $repoParts = $repository.AbsolutePath.Trim('/').Split('/')
        if ($repoParts.Count -lt 2) { throw "Invalid repository URL: $($package.name)" }
        $repoPath = ($repoParts[0] + '/' + $repoParts[1]) -replace '\.git$', ''
        if ($repository.Scheme -ne 'https' -or $repository.Host -ne 'github.com' -or $repository.UserInfo -or $repoPath -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' -or $vcs.git.sha1 -notmatch '^[0-9a-f]{40}$') { throw "Manual license review needed: $($package.name)" }
        $cached = Join-Path $licenseCache ($repoPath.Replace('/', '-') + '-' + $vcs.git.sha1)
        New-Item -ItemType Directory -Force -Path $cached | Out-Null
        $checked = Join-Path $cached '.checked'
        if (-not (Test-Path -LiteralPath $checked)) {
        foreach ($name in @('LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'LICENSE.md', 'LICENSE.txt', 'COPYING', 'NOTICE')) {
            $file = Join-Path $cached $name
            if (Test-Path -LiteralPath $file) { continue }
            $response = Invoke-WebRequest "https://raw.githubusercontent.com/$repoPath/$($vcs.git.sha1)/$name" -SkipHttpErrorCheck
            if ($response.StatusCode -eq 404) { continue }
            if ($response.StatusCode -ne 200) { throw "License download failed: $($package.name)" }
            $text = if ($response.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($response.Content) } else { [string]$response.Content }
            if (-not $text.Trim()) { throw 'Empty license response' }
            [IO.File]::WriteAllText($file, $text)
        }
        Set-Content -LiteralPath $checked -Value $vcs.git.sha1
        }
        $licenseFiles = @(Get-ChildItem -LiteralPath $cached -File | Where-Object Name -ne '.checked')
        $packageRoot = $cached
        $licenseOrigin = "https://github.com/$repoPath/tree/$($vcs.git.sha1)"
        if ($licenseFiles.Count -eq 0) { throw "No license text found for $($package.name) $($package.version)" }
    }
    $noticeDir = Join-Path $bundle "third-party-licenses/$($package.name)-$($package.version)"
    New-Item -ItemType Directory -Force -Path $noticeDir | Out-Null
    foreach ($file in $licenseFiles) {
        $relative = [IO.Path]::GetRelativePath($packageRoot, $file.FullName)
        if ($relative.StartsWith('..') -or [IO.Path]::IsPathRooted($relative)) { throw "License outside package root: $($package.name)" }
        $targetPath = Join-Path $noticeDir $relative
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $targetPath) | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $targetPath -Force
    }
    $notices += [ordered]@{ name=$package.name; version=$package.version; license=$package.license; repository=$package.repository; licenseOrigin=$licenseOrigin }
}
$notices | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $bundle 'DEPENDENCIES.json') -Encoding utf8
[ordered]@{version=$version; revision=$revision; target=$Target; externalEnginesBundled=$false; executableSha256=(Get-FileHash -LiteralPath (Join-Path $bundle 'Dropflip.exe') -Algorithm SHA256).Hash} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $bundle 'BUILD.json') -Encoding utf8
$archive = Join-Path $release "Dropflip-v$version-windows-x64.zip"
Get-ChildItem -LiteralPath $bundle -Recurse -File | ForEach-Object { $_.LastWriteTimeUtc = [DateTime]::new(2000, 1, 1, 0, 0, 0, [DateTimeKind]::Utc) }
Compress-Archive -LiteralPath $bundle -DestinationPath $archive -CompressionLevel Optimal
& $CompilerPath '/Q' "/DAppVersion=$version" "/DBundleDir=$bundle" "/DReleaseDir=$release" (Join-Path $root 'installer/dropflip.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
$installer = Join-Path $release "Dropflip-v$version-windows-x64-setup.exe"
$checksumLines = @($archive, $installer) | ForEach-Object { $hash=Get-FileHash -LiteralPath $_ -Algorithm SHA256; '{0}  {1}' -f $hash.Hash.ToLowerInvariant(), (Split-Path $_ -Leaf) }
[IO.File]::WriteAllText((Join-Path $release 'SHA256SUMS.txt'), ($checksumLines -join "`n") + "`n", [Text.Encoding]::ASCII)
Write-Output "Release artifacts: $release"
