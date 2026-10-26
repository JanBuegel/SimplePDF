# Downloads a prebuilt pdfium.dll (bblanchon/pdfium-binaries) into .\pdfium\bin
# and copies it next to the debug/release executables.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$asset = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "pdfium-win-arm64.tgz" } else { "pdfium-win-x64.tgz" }
New-Item -ItemType Directory -Force pdfium | Out-Null
$tgz = Join-Path "pdfium" $asset
Invoke-WebRequest "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/$asset" -OutFile $tgz
tar -xzf $tgz -C pdfium
Remove-Item $tgz
foreach ($dir in "target\debug", "target\release") {
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item pdfium\bin\pdfium.dll $dir
}
Get-Item pdfium\bin\pdfium.dll
