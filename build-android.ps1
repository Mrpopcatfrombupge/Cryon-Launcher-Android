param(
    [string]$Abi = 'arm64',
    [string]$Api = '29',
    [string]$Profile = 'fast'
)

$ErrorActionPreference = 'Stop'

# Builds the engine and packages the APK. Api is the oldest Android version
# the result runs on (29 = Android 10).

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = $here

. (Join-Path $here 'android-env.ps1') $Abi $Api

$target = switch ($Abi) {
    'arm64'  { 'aarch64-linux-android' }
    'x86_64' { 'x86_64-linux-android' }
    'arm'    { 'armv7-linux-androideabi' }
    default  { throw "unknown abi $Abi (arm64, x86_64 or arm)" }
}

# `fast` is the workspace's optimized, quick-to-link profile.

Push-Location $root
try {
    cargo build -p cryon-client-android --target $target --profile $Profile
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $target" }
} finally { Pop-Location }

& (Join-Path $root 'packaging\android\build-apk.ps1') $Abi $Profile $Api
if ($LASTEXITCODE -ne 0) { throw 'build-apk.ps1 failed' }

$apk = Join-Path $root ('packaging\android\out\cryon-' + $Abi + '.apk')

Write-Host ''
Write-Host ('install: adb install -r ' + $apk)
Write-Host 'launch : adb shell am start -n dev.cryon.client/dev.cryon.client.MainActivity'
