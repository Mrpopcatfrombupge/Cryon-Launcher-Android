param(
    [string]$Adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe",
    [int]$WorldIndex = 0,
    [int]$JoinSeconds = 90,
    [int]$TickSeconds = 45
)

# Verifies the behaviour runtime end to end, on the device, the way a player
# would reach it: launcher -> Worlds -> a world -> ticks reach Java.
#
#   .\verify-behavior.ps1                  # first world of the current instance
#   .\verify-behavior.ps1 -WorldIndex 1    # the second world
#
# PASS needs four things in logcat: the runtime started mods, the engine's
# bridge installed, the player joined, and at least one mod reported a tick.
# A mod that honestly refused is a finding, not a failure — the line says so.

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $Adb)) { throw "adb not found: $Adb" }
# @() on the whole pipeline: a single matching line must stay an array, or
# $devices[0] would be the string's first character, not the line.
$devices = @((& $Adb devices) | Where-Object { $_ -match 'device\s*$' -and $_ -notmatch 'List of' })
if ($devices.Count -eq 0) { throw 'no device online' }
# adb separates serial and state with a tab, which some capture paths lose
# ("emulator-5554device"): take everything before the state word.
$serial = $devices[0].Substring(0, $devices[0].LastIndexOf('device')).Trim()
Write-Host "device : $serial"

function Find-NodeCenter([string]$pattern) {
    # uiautomator dump can refuse while the screen is animating; retry.
    for ($try = 0; $try -lt 12; $try++) {
        $dump = & $Adb -s $serial exec-out uiautomator dump /dev/tty 2>$null
        if ($dump -match "$pattern[^>]*bounds=`"\[(\d+),(\d+)\]\[(\d+),(\d+)\]`"") {
            return @((([int]$Matches[1] + [int]$Matches[3]) / 2),
                     (([int]$Matches[2] + [int]$Matches[4]) / 2))
        }
        Start-Sleep -Milliseconds 800
    }
    return $null
}

function Wait-Logcat([string]$regex, [int]$seconds) {
    $deadline = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $deadline) {
        $line = & $Adb -s $serial logcat -d 2>$null |
            Select-String -Pattern $regex | Select-Object -Last 1
        if ($line) { return "$line" }
        Start-Sleep -Seconds 2
    }
    return $null
}

# Fresh run: no stale evidence from a previous launch.
& $Adb -s $serial shell am force-stop dev.cryon.client | Out-Null
Start-Sleep -Seconds 1
& $Adb -s $serial logcat -c
& $Adb -s $serial shell am start -n dev.cryon.client/dev.cryon.client.MainActivity | Out-Null
Start-Sleep -Seconds 4

$worlds = Find-NodeCenter 'text="Worlds"'
if (-not $worlds) { throw 'Worlds row not found on the launcher home' }
& $Adb -s $serial shell input tap $worlds[0] $worlds[1] | Out-Null
Start-Sleep -Seconds 2

$worldDumps = 0
$world = $null
while (-not $world -and $worldDumps -lt 12) {
    $dump = & $Adb -s $serial exec-out uiautomator dump /dev/tty 2>$null
    $names = [regex]::Matches($dump,
        'world_name"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"')
    if ($names.Count -gt $WorldIndex) {
        $m = $names[$WorldIndex]
        $world = @((([int]$m.Groups[1].Value + [int]$m.Groups[3].Value) / 2),
                   (([int]$m.Groups[2].Value + [int]$m.Groups[4].Value) / 2))
    } else { $worldDumps++; Start-Sleep -Milliseconds 800 }
}
if (-not $world) { throw "world $WorldIndex not found on the Worlds screen" }
& $Adb -s $serial shell input tap $world[0] $world[1] | Out-Null
Write-Host ('world  : tapped row ' + $WorldIndex + ', loading...')

$joined = Wait-Logcat 'joined the game' $JoinSeconds
if (-not $joined) { throw "player did not join within ${JoinSeconds}s" }
Write-Host "join   : $joined"

$runtime = Wait-Logcat 'behaviour runtime: \d+ mod' $JoinSeconds
$bridge  = Wait-Logcat 'behaviour bridge installed' $JoinSeconds
$ticks   = Wait-Logcat 'tick \d+ reached Java' $TickSeconds
$refused = & $Adb -s $serial logcat -d 2>$null |
    Select-String -Pattern 'behaviour could not run' | Select-Object -Last 1

Write-Host ''
Write-Host '--- behaviour verification -------------------------------------'
$pass = $true
foreach ($check in @(
        @('runtime started', $runtime),
        @('engine bridge',   $bridge),
        @('tick reached Java', $ticks))) {
    if ($check[1]) { Write-Host ('PASS  ' + $check[0] + ': ' + $check[1]) }
    else { Write-Host ('FAIL  ' + $check[0]); $pass = $false }
}
if ($runtime -match 'behaviour runtime: (\d+) mod\(s\) running, (\d+) refused') {
    Write-Host ("mods  : " + $Matches[1] + " running, " + $Matches[2] + " refused")
}
if ($refused) { Write-Host "note  : $refused" }
Write-Host '----------------------------------------------------------------'
if ($pass) { Write-Host 'VERDICT: PASS - the engine tick reaches Java listeners in-world' }
else { Write-Host 'VERDICT: FAIL'; exit 1 }
