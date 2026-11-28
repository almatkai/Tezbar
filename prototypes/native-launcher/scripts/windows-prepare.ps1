# Task-owned staging only. Never uses C:\Users\User\Desktop\Tezbar.
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$stage=Join-Path $env:TEMP ('tezbar-native-launcher-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
$download=Join-Path $stage 'rustup-init.exe'
$url='https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe'
Invoke-WebRequest -TimeoutSec 45 -UseBasicParsing $url -OutFile $download
Invoke-WebRequest -TimeoutSec 45 -UseBasicParsing ($url+'.sha256') -OutFile ($download+'.sha256')
$expected=((Get-Content ($download+'.sha256') -Raw) -split '\s+')[0]
if((Get-FileHash $download -Algorithm SHA256).Hash.ToLower() -ne $expected.ToLower()){throw 'rustup download hash mismatch'}
$rustup=Join-Path $stage 'rustup.exe'
Copy-Item $download $rustup
& $rustup toolchain install 1.98.1 --profile minimal
if($LASTEXITCODE -ne 0){throw 'toolchain installation failed'}
& $rustup toolchain list
Write-Output ('TEZBAR_NATIVE_STAGE='+$stage)
