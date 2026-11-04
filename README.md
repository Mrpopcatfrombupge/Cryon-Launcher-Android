# Cryon Launcher Android

Android launcher for Cryon, a Rust reimplementation of Minecraft: Java Edition.

Pick a version on the Versions screen and the launcher downloads the official game files from Mojang, verified by SHA-1. The game runs on the native Rust engine in the same app. No Java runtime on the device.

> Unofficial. Not affiliated with Mojang or Microsoft. You need a legitimate copy of Minecraft: Java Edition.

## Install

Get the APK from [Releases](../../releases). One APK for arm64-v8a and armeabi-v7a, Android 10 or later.

## Features

- Version manager with SHA-1 verified, resumable downloads
- Native Rust engine, no JVM on the device
- Touch controls with a configurable layout
- Multiple instances, worlds and servers
- Fabric-style mod support: drop a mod jar into the instance's mods folder. Mod content (blocks, items, recipes, textures, worldgen) is converted to native data, and mod entrypoints using the Fabric lifecycle interfaces run in-game.
- In-app updater: Home → Updates checks GitHub Releases and installs a newer APK

## Build

Requires Rust with the Android targets, NDK r27c, Android SDK build-tools 34.0.0, JDK 17+, PowerShell.

```powershell
.\build-android.ps1                                # arm64 APK
.\build-android.ps1 -Abi arm                       # 32-bit arm APK
.\packaging\android\build-apk.ps1 -Abi arm64,arm   # universal APK
```

The build does not use Gradle. The APK is assembled directly with aapt, javac, d8, zipalign and apksigner.

## Repository layout

- `crates/` the Rust engine
- `packaging/android/` the launcher app
- `third_party/` vendored wgpu-hal and naga fixes
- `build-android.ps1` build entry point

## Credits

- r1xm: launcher, packaging, mod porter, behavior runtime
- Ludovic Marie and the Horizon contributors: the engine, MIT licensed
- gfx-rs: wgpu and naga
- Fabric: the mod lifecycle interfaces

## License

MIT. See [LICENSE](LICENSE).
