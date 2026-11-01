param([string]$Version = '0.1.0')
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^[0-9A-Za-z.-]+$') { throw 'Version must be a simple filename-safe version string.' }
$repo = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repo
try {
    cargo build --release --locked --bins
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    cargo about generate --all-features --locked docs/licenses.hbs -o THIRD_PARTY_NOTICES.html
    if ($LASTEXITCODE -ne 0) { throw 'Notice generation failed. Install cargo-about 0.9.3 first.' }
    $hostTarget = ((rustc -vV | Select-String '^host: ').Line -replace '^host: ', '').Trim()
    if ($hostTarget -notmatch 'windows') { throw 'This script packages Windows binaries. Use the CI workflow for other operating systems.' }
    $packageName = "big-picture-$Version-$hostTarget"
    $destination = Join-Path $repo "dist/$packageName"
    if (Test-Path -LiteralPath $destination) { throw "Package directory exists: $destination. Choose a new version or remove the previous package deliberately." }
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Copy-Item target/release/big-picture.exe,target/release/big-picture-gui.exe,LICENSE,README.md,THIRD_PARTY_NOTICES.html,CONTRIBUTING.md,CHANGELOG.md -Destination $destination
    Copy-Item docs -Destination $destination -Recurse
    Copy-Item docs/font-licenses -Destination $destination -Recurse
    $archive = "$destination.zip"
    Compress-Archive -Path "$destination/*" -DestinationPath $archive
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $packageName.zip" | Set-Content -LiteralPath "$archive.sha256" -Encoding utf8
    Write-Output $archive
} finally { Pop-Location }
