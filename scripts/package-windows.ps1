# Builds the release exe and packs a Velopack installer into .\dist\releases.
# Needs: Rust, pdfium.dll (scripts\fetch-pdfium.ps1), .NET 8 SDK and `dotnet tool install -g vpk`.
# Native tools write progress to stderr; failures are detected via $LASTEXITCODE instead.
$ErrorActionPreference = "Continue"
Set-Location (Join-Path $PSScriptRoot "..")

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$pack = "dist\pack"
Remove-Item -Recurse -Force $pack -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $pack | Out-Null
Copy-Item target\release\simplepdf.exe, pdfium\bin\pdfium.dll $pack

vpk pack --packId SimplePDF --packVersion $version --packTitle SimplePDF `
    --packDir $pack --mainExe simplepdf.exe --outputDir dist\releases
if ($LASTEXITCODE -ne 0) { throw "vpk pack failed" }
Get-ChildItem dist\releases
