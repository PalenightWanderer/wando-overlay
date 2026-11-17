$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Visual Studio C++ tools are required.' }
$cmake = Join-Path $vs 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe'
if (-not (Test-Path $cmake)) { $cmake = (Get-Command cmake -ErrorAction Stop).Source }
& $cmake -S $root -B "$root/build" -A x64
if ($LASTEXITCODE) { throw 'CMake configuration failed.' }
& $cmake --build "$root/build" --config Release --parallel
if ($LASTEXITCODE) { throw 'Native build failed.' }
Push-Location $root
try { cargo build; if ($LASTEXITCODE) { throw 'Rust build failed.' } } finally { Pop-Location }
