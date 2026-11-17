# Builds a release copy that runs from its own folder: dist\Wander Overlay.
# Start it with "Wander Overlay.exe"; it waits for SteamVR and reconnects.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
& "$PSScriptRoot/build.ps1"
Push-Location $root
try { cargo build --release; if ($LASTEXITCODE) { throw 'Release build failed.' } } finally { Pop-Location }
$out = Join-Path $root 'dist/Wander Overlay'
if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item $out -ItemType Directory | Out-Null
Copy-Item "$root/target/release/wander-overlay.exe" "$out/Wander Overlay.exe"
Copy-Item "$root/build/Release/*.dll" $out
Copy-Item "$root/build/Release/assets" $out -Recurse
# Ultralight's Free license requires shipping its EULA and NOTICES.md with the DLLs.
$licenses = Join-Path $out 'licenses'
New-Item "$licenses/Ultralight", "$licenses/OpenVR" -ItemType Directory | Out-Null
Copy-Item "$root/ul-sdk-2.0.0-beta.2-win-x64-free/license/EULA.txt", "$root/ul-sdk-2.0.0-beta.2-win-x64-free/license/NOTICES.md" "$licenses/Ultralight"
Copy-Item "$root/vendor/openvr/LICENSE" "$licenses/OpenVR"
Copy-Item "$root/LICENSE" "$licenses/Wander Overlay LICENSE.txt"
Write-Host "Packaged: $out"
