param([switch]$SkipChecks)
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    if (-not $SkipChecks) {
        cargo fmt --all -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed.' }
        cargo clippy --all-targets --all-features --locked -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
        cargo test --all-targets --all-features --locked
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
    }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read package metadata.' }
    $version = ($metadata.packages | Where-Object name -EQ 'branchroom').version
    $hostTriple = (rustc -vV | Select-String '^host: ').ToString().Substring(6)
    if ($hostTriple -ne 'x86_64-pc-windows-msvc') { throw "Expected Windows x64 MSVC, got $hostTriple." }
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
    $packageName = "Branchroom-$version-Windows-x64-$stamp"
    $package = Join-Path (Join-Path $PWD 'dist') $packageName
    New-Item -ItemType Directory -Path $package | Out-Null
    Copy-Item -LiteralPath target/release/branchroom.exe -Destination (Join-Path $package 'branchroom.exe')
    Copy-Item -LiteralPath README.md,LICENSE -Destination $package
    Copy-Item -LiteralPath docs -Destination (Join-Path $package 'docs') -Recurse
    $hashes = Get-ChildItem -LiteralPath $package -File -Recurse | ForEach-Object {
        [ordered]@{path=$_.FullName.Substring($package.Length+1).Replace('\','/'); sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash; bytes=$_.Length}
    }
    $manifest = [ordered]@{name='Branchroom';version=$version;target=$hostTriple;createdUtc=(Get-Date).ToUniversalTime().ToString('o');signed=$false;files=@($hashes)}
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $package 'manifest.json') -Encoding UTF8
    $zip = "$package.zip"
    Compress-Archive -LiteralPath $package -DestinationPath $zip
    [ordered]@{version=$version;directory=$package;executable=(Join-Path $package 'branchroom.exe');archive=$zip;archiveSha256=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash} | ConvertTo-Json | Set-Content -LiteralPath dist/latest.json -Encoding UTF8
    # A unique package stays available when an older executable is running.
    try { Copy-Item -LiteralPath (Join-Path $package 'branchroom.exe') -Destination dist/branchroom.exe -ErrorAction Stop }
    catch { Write-Warning 'The previous dist/branchroom.exe is in use. Use the new versioned package.' }
    Write-Host "Built $package"
    Write-Host "Archive $zip"
} finally {
    Pop-Location
}
