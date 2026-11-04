param(
    [string[]]$Abi = @('arm64'),
    [string]$Profile = 'fast',
    [string]$Api = '29',
    [switch]$NoJava
)

$ErrorActionPreference = 'Stop'

# Builds an installable Cryon APK from a cargo-built Android .so.
#
#   .\build-apk.ps1                          # arm64, the default
#   .\build-apk.ps1 -Abi x86_64              # LDPlayer and most desktop emulators
#   .\build-apk.ps1 -Abi arm64,x86_64        # one APK carrying both ("universal")
#   .\build-apk.ps1 -Abi arm64 -Api 26       # the same library with an API-26 floor
#   .\build-apk.ps1 -NoJava                  # the engine alone, no launcher screen
#
# One .so per ABI: cargo has to have built each one first (build-android.ps1 does
# one ABI at a time, because the target directory serializes the builds anyway).
#
# There is no Gradle here on purpose. The APK is two activities, a handful of Java
# classes and one native library, so aapt + javac + a dexer + zipalign + apksigner
# is the whole job, and it is the same job this script already did when the app had
# no code in it.
#
# The dexer is the one thing that is not build-tools' own. d8 8.2.2, which is what
# build-tools 34 ships, stops with an internal NullPointerException on any class
# that captures an enclosing instance - every anonymous class and every non-static
# inner class, which is most of what an Android UI is made of. It is a defect of
# that build and not of the code: R8 8.5.35, which is what AGP 8.5 uses, dexes the
# same class files without complaint, so that jar is fetched once and called
# through java -cp.
#
# The .so goes into the archive at lib/<abi>/ by hand, not through aapt: aapt
# has no option for a library directory, and passing the staging folder as -A
# puts it at assets/lib/<abi>/ instead, where the classloader cannot see it and
# NativeActivity dies with "unable to find native library".
#
# $Api is both the level the library was linked against (android-env.ps1's
# --target=<triple>$Api) and the minSdkVersion this script packs. They are the
# same number because they are the same fact: the oldest device the binaries
# start on. The manifest in this folder is not edited - a copy with that one
# attribute replaced is what gets compiled in, so an APK can never promise a
# device it will not load on, whatever the source file says. The reason is
# concrete: sound is *linked* against libaaudio (ndk-sys' #[link(name =
# "aaudio")]), so the finished library carries a DT_NEEDED entry for a .so that
# does not exist on Android 7 and below. minSdkVersion=21 against that binary is
# a promise the installer honours and the loader breaks: it crashes on launch
# before any Cryon code runs.
#
# Strings are single-quoted and joined with + throughout, and arguments are
# passed as splatted arrays: Windows PowerShell 5.1 mis-parses this file if any
# non-ASCII character or line continuation slips in.

$here   = Split-Path -Parent $MyInvocation.MyCommand.Path
$root   = (Resolve-Path (Join-Path $here '..\..')).Path
$sdk    = Join-Path $env:LOCALAPPDATA 'Android\Sdk'
$bt     = Join-Path $sdk 'build-tools\34.0.0'
$out    = Join-Path $here 'out'
$jar    = Join-Path $sdk 'platforms\android-34\android.jar'
$manifest = Join-Path $here 'AndroidManifest.xml'
$res      = Join-Path $here 'res'
$src      = Join-Path $here 'src'
$gen      = Join-Path $out 'gen'
$classes  = Join-Path $out 'classes'
$dex      = Join-Path $out 'dex'

# The Java tools and the dexer both need a JDK, and the `java` on this machine's
# PATH is a 1.8 JRE: too old for javac of this vintage and too old for R8. The
# Microsoft JDK that builds the desktop launcher is put first for the whole
# script, and android.jar and the R8 jar are copied to a path with no space in it
# because d8.bat and apksigner.bat pass their arguments through cmd unquoted.
$jdk   = 'C:\Program Files\Microsoft\jdk-25.0.3.9-hotspot\bin'
$tools = 'C:\cryon-tools'
$r8    = Join-Path $tools 'r8-8.5.35.jar'
$env:JAVA_HOME = Split-Path -Parent $jdk
$env:Path = $jdk + ';' + $env:Path

$apiLevels = $Api -as [int]
if (-not $apiLevels -or $apiLevels -lt 21 -or $apiLevels -gt 35) {
    throw "API level $Api is outside what NDK r27c knows (21 to 35)"
}

$abiMap = @{
    'arm64'     = @{ target = 'aarch64-linux-android';   lib = 'arm64-v8a' }
    'x86_64'    = @{ target = 'x86_64-linux-android';    lib = 'x86_64' }
    'arm'       = @{ target = 'armv7-linux-androideabi'; lib = 'armeabi-v7a' }
}
$sos = @()
foreach ($a in $Abi) {
    if (-not $abiMap.ContainsKey($a)) { throw ('unknown abi: use arm64, x86_64 or arm, got ' + $a) }
    $candidate = Join-Path $root ('target\' + $abiMap[$a].target + '\' + $Profile + '\libcryon_client_android.so')
    if (-not (Test-Path $candidate)) {
        throw ('build it first: cargo build -p cryon-client-android --target ' +
               $abiMap[$a].target + ' --profile ' + $Profile)
    }
    $sos += [pscustomobject]@{ Abi = $a; Lib = $abiMap[$a].lib; Path = $candidate }
}

$unsigned = Join-Path $out 'cryon-unsigned.apk'
$aligned  = Join-Path $out 'cryon-aligned.apk'
$name = if ($sos.Count -eq 1) { $sos[0].Abi } else { 'universal' }
$final    = Join-Path $out ('cryon-' + $name + '.apk')
New-Item -ItemType Directory -Path $out -Force | Out-Null

# The manifest to pack: the one in this folder with minSdkVersion set to $Api.
$packagedManifest = Join-Path $out 'AndroidManifest.xml'
$source = Get-Content $manifest -Raw
if ($source -notmatch 'minSdkVersion="(\d+)"') {
    throw 'AndroidManifest.xml has no minSdkVersion to align with the build'
}
$declared = [int]$Matches[1]
if ($declared -ne $apiLevels) {
    Write-Host ('manifest says minSdkVersion=' + $declared + '; packing ' + $apiLevels +
                ' (what these binaries load on). Fix packaging\android\AndroidManifest.xml too.')
}
Set-Content -Path $packagedManifest -Encoding ASCII -Value ($source -replace
    'minSdkVersion="\d+"', ('minSdkVersion="' + $apiLevels + '"'))

# Two things the manifest should not be doing, reported rather than patched:
# they are the source file's business, and the fix belongs in one place.
if ($source -match 'READ_EXTERNAL_STORAGE|WRITE_EXTERNAL_STORAGE') {
    Write-Host 'note: the manifest asks READ/WRITE_EXTERNAL_STORAGE. The game only ever'
    Write-Host '      writes Android/data/dev.cryon.client/files, its own directory, which'
    Write-Host '      has needed no permission since API 19. Drop both lines.'
}
if ($source -match 'requestLegacyExternalStorage') {
    Write-Host 'warning: requestLegacyExternalStorage is set. It does nothing from API 30 on'
    Write-Host '         and is not read anywhere in this app: the game should not depend on it.'
}

foreach ($s in $sos) { Write-Host ('packing for ' + $s.Lib) }

$aaptArgs = @('package', '-f', '-M', $packagedManifest, '-I', $jar)
if (Test-Path $res) { $aaptArgs += @('-S', $res) }

# The behaviour runtime's own dexer: d8 dexed, carried as an asset. On the
# device the runtime runs it through dalvikvm to turn a mod's Java (the jar
# the porter kept beside the port) into ART-ready dex at game start. Made
# here from the same R8 jar the build itself dexes with — a tool that can
# produce itself — and cached beside the R8 download, since a 7 MB self-dex
# costs seconds that a rebuild should not spend again.
if (-not $NoJava) {
    if (-not (Test-Path $r8)) {
        Write-Host 'fetching the dexer'
        New-Item -ItemType Directory -Force -Path $tools | Out-Null
        Invoke-WebRequest -UseBasicParsing `
            'https://dl.google.com/dl/android/maven2/com/android/tools/r8/8.5.35/r8-8.5.35.jar' `
            -OutFile $r8
    }
    $selfdexDir = Join-Path $out 'dexer-self'
    $selfdex = Join-Path $out 'dexed-d8.dex'
    $stale = -not (Test-Path $selfdex) -or
        ((Get-Item $r8).LastWriteTime -gt (Get-Item $selfdex).LastWriteTime)
    if ($stale) {
        Write-Host 'dexing the dexer'
        # d8's --output wants an existing directory; it is left in place (no
        # deletes in a build) and overwritten on the next stale pass.
        New-Item -ItemType Directory -Force -Path $selfdexDir | Out-Null
        & (Join-Path $jdk 'java.exe') -Xmx2g -cp $r8 com.android.tools.r8.D8 `
            --release --min-api $Api --output $selfdexDir $r8
        if ($LASTEXITCODE -ne 0) { throw 'self-dexing the dexer failed' }
        Move-Item -Force (Join-Path $selfdexDir 'classes.dex') $selfdex
    }
    $assets = Join-Path $out 'assets'
    New-Item -ItemType Directory -Force -Path $assets | Out-Null
    Copy-Item -Force $selfdex (Join-Path $assets 'dexed-d8.dex')
    $aaptArgs += @('-A', $assets)
}
if (-not $NoJava) {
    # -J writes R.java, the class every layout and colour in res/ is reached
    # through. It is generated, not checked in, because it is a function of the
    # resources and a stale one compiles into a screen that shows the wrong thing.
    Remove-Item -Recurse -Force $gen -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $gen | Out-Null
    $aaptArgs += @('-J', $gen)
}
$aaptArgs += @('-F', $unsigned)
& (Join-Path $bt 'aapt.exe') @aaptArgs | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'aapt package failed' }

if (-not $NoJava) {
    # The launcher's Java. Compiled against the platform jar only: no AndroidX,
    # no support library, so there is nothing to resolve and no Gradle to resolve
    # it with, and the app is the same size as the code it is made of.
    Write-Host 'javac'
    Remove-Item -Recurse -Force $classes -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $classes | Out-Null
    $sources = @()
    $sources += Get-ChildItem -Recurse -Filter *.java $src | ForEach-Object { $_.FullName }
    $sources += Get-ChildItem -Recurse -Filter *.java $gen | ForEach-Object { $_.FullName }
    if ($sources.Count -eq 0) { throw 'no Java sources under ' + $src }
    # javac's notes and warnings go on standard error, which Windows PowerShell 5.1
    # turns into a terminating NativeCommandError under `Stop` - so a build dies on
    # the line `Note: Some input files use or override a deprecated API`, which is
    # not a failure and says nothing about the APK. The preference is relaxed for the
    # call and the exit code is read afterwards, which is the only real answer.
    $preference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & (Join-Path $jdk 'javac.exe') -nowarn -source 11 -target 11 -classpath $jar `
        -d $classes $sources 2>&1 | ForEach-Object { "$_" }
    $compiled = $LASTEXITCODE
    $ErrorActionPreference = $preference
    if ($compiled -ne 0) { throw 'javac failed' }

    # One jar in, not a list of class files: every inner class is named
    # Outer$Inner.class and the dollar sign does not survive cmd.
    $classesJar = Join-Path $out 'classes.jar'
    Remove-Item -Force $classesJar -ErrorAction SilentlyContinue
    Push-Location $classes
    try {
        & (Join-Path $jdk 'jar.exe') --create --file $classesJar .
        if ($LASTEXITCODE -ne 0) { throw 'jar failed' }
    } finally { Pop-Location }

    Write-Host 'd8'
    Remove-Item -Recurse -Force $dex -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $dex | Out-Null
    & (Join-Path $jdk 'java.exe') -cp $r8 com.android.tools.r8.D8 `
        --min-api $Api --lib $jar --output $dex $classesJar
    if ($LASTEXITCODE -ne 0) { throw 'd8 failed' }
    if (-not (Test-Path (Join-Path $dex 'classes.dex'))) { throw 'd8 produced no classes.dex' }
}

# The libraries, stored under the path the installer extracts from. Compressed on
# purpose: the manifest sets extractNativeLibs, so Android unpacks it at install
# and the APK stays a fraction of the .so's size.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::Open($unsigned, 'Update')
try {
    foreach ($s in $sos) {
        $entryPath = 'lib/' + $s.Lib + '/libcryon_client_android.so'
        [void]$zip.CreateEntry($entryPath, [System.IO.Compression.CompressionLevel]::Optimal)
        $src = [System.IO.File]::OpenRead($s.Path)
        $dst = $zip.GetEntry($entryPath).Open()
        try { $src.CopyTo($dst) } finally { $dst.Dispose(); $src.Dispose() }
    }
    if (-not $NoJava) {
        # The name is fixed by the format: a dex anywhere else in the archive is
        # simply not looked at, and the app installs and dies on launch with
        # "Class not found" if it is missing or misnamed.
        $dexPath = Join-Path $dex 'classes.dex'
        [void]$zip.CreateEntry('classes.dex', [System.IO.Compression.CompressionLevel]::Optimal)
        $src = [System.IO.File]::OpenRead($dexPath)
        $dst = $zip.GetEntry('classes.dex').Open()
        try { $src.CopyTo($dst) } finally { $dst.Dispose(); $src.Dispose() }
    }
} finally { $zip.Dispose() }

& (Join-Path $bt 'zipalign.exe') -f -p 4 $unsigned $aligned | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'zipalign failed' }

# A debug key, created once. Fine for local testing; never ship this one.
$keystore = Join-Path $out 'cryon-debug.keystore'
if (-not (Test-Path $keystore)) {
    Write-Host 'creating a debug signing key'
    $keyArgs = @(
        '-genkeypair', '-noprompt',
        '-keystore', $keystore,
        '-storepass', 'android',
        '-keypass', 'android',
        '-alias', 'cryon',
        '-keyalg', 'RSA',
        '-keysize', '2048',
        '-validity', '10000',
        '-dname', 'CN=Cryon Debug, OU=Dev, O=Cryon, L=Unknown, ST=Unknown, C=US'
    )
    & keytool.exe @keyArgs | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'keytool failed: is a JDK on PATH?' }
}

$signArgs = @('sign', '--ks', $keystore, '--ks-pass', 'pass:android', '--out', $final, $aligned)
& (Join-Path $bt 'apksigner.bat') @signArgs | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'apksigner failed' }

# What was actually built, read back rather than assumed: the APK's own declared
# floor, and the libraries the loader will insist on finding on the device. The
# second is the real minSdk: a DT_NEEDED for libaaudio.so means no Android 7.
$badging = & (Join-Path $bt 'aapt.exe') dump badging $final 2>$null | Out-String
foreach ($line in ($badging -split "`n")) {
    if ($line -match "sdkVersion:|targetSdkVersion:|native-code:") { Write-Host ('apk: ' + $line.Trim()) }
}
$readelf = Join-Path 'C:\cryon-ndk' 'toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-readelf.exe'
if (Test-Path $readelf) {
    foreach ($s in $sos) {
        $needed = @(& $readelf -d $s.Path 2>$null | Out-String -Stream |
            Where-Object { $_ -match 'NEEDED' } | ForEach-Object { ($_ -replace '.*\[', '') -replace '\].*', '' })
        Write-Host ($s.Lib + ' needs: ' + ($needed -join ' '))
    }
}

foreach ($s in $sos) {
    Write-Host ('so      : ' + $s.Lib + ' ' +
                [math]::Round((Get-Item $s.Path).Length / 1MB, 1) + ' MB')
}
$mb = [math]::Round((Get-Item $final).Length / 1MB, 1)
Write-Host ('apk     : ' + $final)
Write-Host ('size    : ' + $mb + ' MB')
Write-Host ('floor   : API ' + $apiLevels)
Write-Host ('install : adb install -r ' + $final)
if ($NoJava) {
    Write-Host 'launch  : adb shell am start -n dev.cryon.client/android.app.NativeActivity'
} else {
    Write-Host 'launch  : adb shell am start -n dev.cryon.client/.MainActivity'
    Write-Host '          The game is started by PLAY. NativeActivity stays'
    Write-Host '          startable on its own for when the launcher is the thing'
    Write-Host '          being tested.'
}
