# Sets the shell environment so cargo can build for Android with the NDK.
#
# Usage:  . .\android-env.ps1            # then cargo build -p cryon-client-android --target aarch64-linux-android
#         . .\android-env.ps1 x86_64     # for an x86_64 emulator image (LDPlayer)
#         . .\android-env.ps1 arm64 26   # an experiment: reach Android 8.0 and up
#
# The second argument is the API level everything is built against, and it is the
# same number that belongs in the manifest's minSdkVersion: it is the oldest
# device these binaries can start on. packaging\android\build-apk.ps1 reads that
# floor back out of the finished library, so the two cannot drift apart.
#
# `param` comes first because PowerShell allows nothing but comments above it.
param(
    [string]$Arch = 'arm64',
    [string]$Api = '29'
)

$ErrorActionPreference = 'Stop'

$ndk = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Android') -Directory -Filter 'android-ndk-*' |
    Sort-Object Name -Descending | Select-Object -First 1
if (-not $ndk) { throw "no android-ndk-* folder found; extract ndk.zip first" }

# cc-rs splits CC on spaces before it runs the command, so a toolchain under
# a profile path with spaces arrives as two arguments and the build dies with
# "is not recognized as an internal or external command". The fix is a path with
# no space in it: a junction to the NDK. A junction needs no administrator
# rights, which a symlink on this machine does.
$link = 'C:\cryon-ndk'
if (-not (Test-Path (Join-Path $link 'toolchains'))) {
    if (Test-Path $link) { Remove-Item $link -Force }  # a stale junction, not the NDK itself
    New-Item -ItemType Junction -Path $link -Target $ndk.FullName | Out-Null
}

$bin = Join-Path $link 'toolchains\llvm\prebuilt\windows-x86_64\bin'
if (-not (Test-Path (Join-Path $bin 'clang.exe'))) { throw "no NDK toolchain at $bin" }
$sysroot = Join-Path (Split-Path $bin) 'sysroot'

$target = switch ($Arch) {
    'arm64'  { 'aarch64-linux-android' }
    'x86_64' { 'x86_64-linux-android' }
    'arm'    { 'armv7-linux-androideabi' }
    default  { throw "unknown arch $Arch (arm64, x86_64 or arm)" }
}

# Two different programs, for two different jobs.
#
# Compiling wants the bare clang.exe: cc-rs passes --target itself, and it cannot
# start a .cmd wrapper at all.
$env:CC_x86_64_linux_android   = Join-Path $bin 'clang.exe'
$env:CXX_x86_64_linux_android  = Join-Path $bin 'clang++.exe'
$env:CC_aarch64_linux_android  = Join-Path $bin 'clang.exe'
$env:CXX_aarch64_linux_android = Join-Path $bin 'clang++.exe'
$env:CC_armv7_linux_androideabi  = Join-Path $bin 'clang.exe'
$env:CXX_armv7_linux_androideabi = Join-Path $bin 'clang++.exe'
$env:AR_x86_64_linux_android   = Join-Path $bin 'llvm-ar.exe'
$env:AR_aarch64_linux_android  = Join-Path $bin 'llvm-ar.exe'
$env:AR_armv7_linux_androideabi = Join-Path $bin 'llvm-ar.exe'

# The API level to build against: the floor of the devices the result starts on.
#
# Two facts set it.
#
#  * Sound is `cpal`'s AAudio host, and `ndk-sys` declares it with
#    `#[link(name = "aaudio")]`, so `-l aaudio` is a link flag rather than a
#    `dlopen` at run time: below the level where the sysroot has the stub
#    library, the build stops with "unable to find library -laaudio" instead of
#    producing something that might work without sound. In NDK r27c that stub
#    exists for every ABI from API 26 up (Android 8.0), and `aaudio/AAudio.h`
#    marks each of the fourteen AAudio functions the finished library calls
#    `__INTRODUCED_IN(26)`. So 26 is the real audio floor, not 29.
#  * 29 is the level this game has actually been booted with, and everything
#    above the floor that has not been tried on a phone is a claim, not a fact.
#    See ..\docs\ANDROID-COMPATIBILITY.md before lowering it.
#
# The check below catches the one mistake that costs a whole build: a level with
# no stub, found only after the dependency tree has compiled.
$apiLevels = $Api -as [int]
if (-not $apiLevels -or $apiLevels -lt 21 -or $apiLevels -gt 35) {
    throw "API level $Api is outside what NDK r27c knows (21 to 35)"
}
# The sysroot names the 32-bit arm tree with clang's older triple; rustc and
# the linker accept both forms, only this stub lookup needed the mapping.
$sysrootTriple = if ($target -eq 'armv7-linux-androideabi') { 'arm-linux-androideabi' } else { $target }
$stubDir = Join-Path (Join-Path $sysroot 'usr/lib') ($sysrootTriple + '\' + $apiLevels)
if (-not (Test-Path $stubDir)) {
    throw "no sysroot for $target at API $Api (21 to 35 in NDK r27c)"
}
if (-not (Test-Path (Join-Path $stubDir 'libaaudio.so'))) {
    throw ("API $Api for $target has no libaaudio.so in the sysroot: sound is linked, not " +
           'loaded at run time, so the build would fail after every dependency compiled. ' +
           'Use 26 or above (see docs/ANDROID-COMPATIBILITY.md for what a below-26 build ' +
           'would take).')
}
if ($apiLevels -lt 29) {
    Write-Warning ("API $Api has never been booted. The audio floor is 26 and that is " +
                   'reasoned, not tested - 29 is the only level this build has run on.')
}

# Linking wants clang.exe with the two things the .cmd wrapper would have said:
# the target *with its API level*, and the sysroot. Without the target clang
# picks the host and the COFF linker, and rejects every ELF option rustc passes
# ("lld: error: unknown argument: --version-script"). Without the sysroot ld.lld
# cannot find crtbegin_so.o, -landroid or -llog.
#
# The API level matters on its own: the audio path reaches for libaaudio, which
# is in the sysroot from API 26 up (Android 8.0), so an android24 link fails with
# "unable to find library -laaudio" - and, because the dependency is a link flag
# and not a dlopen, an API 24 *device* would not even load the library.
$env:RUSTFLAGS = "-C link-arg=--target=$target$Api -C link-arg=--sysroot=$sysroot"
# Set-Item, not $env:(...): PowerShell cannot build an environment variable name
# from an expression in the $env: form. The name must be uppercased: cargo warns
# and ignores CARGO_TARGET_x86_64_linux_android_LINKER, then goes looking for a
# linker called "cc" and fails at the very last step of a long build.
$var = 'CARGO_TARGET_' + ($target -replace '-', '_').ToUpper() + '_LINKER'
Set-Item -Path "env:$var" -Value (Join-Path $bin 'clang.exe')

Write-Host "NDK     : $($ndk.FullName)"
Write-Host "via     : $link"
Write-Host "target  : $target (API $Api)"
Write-Host "cc      : $env:CC_x86_64_linux_android"
Write-Host "link    : $env:RUSTFLAGS"
