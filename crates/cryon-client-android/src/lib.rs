//! The Android entry point.
//!
//! Android does not run an executable: `NativeActivity` loads this crate as a
//! `libcryon_client_android.so` and calls `android_main` with the process's
//! `AndroidApp`. Everything the game needs that only the system can supply —
//! the app handle, and with it the surface, input queue and file paths — is
//! handed to the client before the game loop starts.
//!
//! The game itself is unchanged. The one Android-only decision is where its
//! files live: there is no command line here, so the launcher's `--resources`
//! and `--game-dir` become the environment variables the client also reads,
//! pointed at the app's own directories.

use std::path::PathBuf;

/// Called by `NativeActivity` through the Java glue. The name and linkage are
/// fixed by android-activity; the body is ordinary Rust.
///
/// android-activity passes `AndroidApp` across the C boundary itself and does
/// not mark it `#[repr(C)]`, so the layout warning is the upstream API's, not
/// something this shim can fix without changing the signature it requires.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(app: android_activity::AndroidApp) {
    // Before anything else: without this the game's own log calls, and the
    // error this function reports when it will not start, go nowhere at all.
    log::set_boxed_logger(Box::new(Logcat)).ok();
    log::set_max_level(log::LevelFilter::Info);

    cryon_client::telemetry::mark_start();
    cryon_client_input::android::set_app(app.clone());
    install_orientation_handler();
    behavior::install();
    point_at_app_dirs(&app);
    let home = cryon_client::logging::game_home();
    cryon_client::crash::install(home.clone(), "client");
    if let Err(error) = cryon_client::game::run() {
        log::error!("Cryon: {error}");
        cryon_client::crash::report_error(&home, "client", &*error);
    }
}

/// Gives the client a way to turn the screen, which is a Java call.
///
/// The launcher reads top to bottom and belongs in portrait; the game wants the
/// wide view, so PLAY asks for landscape and the phone is turned rather than the
/// app stretched. `Activity.setRequestedOrientation` has no C or Rust entry
/// point, so this reaches it through JNI — here, because this is the only crate
/// in the game that makes foreign calls and the ones below forbid unsafe code.
///
/// Taking the whole screen happens in the same breath, because the two are asked
/// for at the same moment: the theme deliberately draws under the system bars so
/// a notch is not answered with a black band, which leaves the phone's own back,
/// home and recents glyphs painted over whatever the launcher put at the bottom.
/// On Android 11 and later `View.setSystemUiVisibility` is ignored, so this uses
/// `WindowInsetsController`, and the bars come back on a swipe from the edge
/// rather than being gone forever.
#[cfg(target_os = "android")]
fn install_orientation_handler() {
    use cryon_client_input::android::Orientation;

    // WindowInsets.Type.NAVIGATION_BARS.
    const NAVIGATION_BARS: i32 = 2;
    // WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE.
    const SHOW_ON_SWIPE: i32 = 2;

    cryon_client_input::android::set_orientation_handler(move |orientation| {
        // Activity.SCREEN_ORIENTATION_PORTRAIT and SCREEN_ORIENTATION_SENSOR_LANDSCAPE.
        let value = match orientation {
            Orientation::Portrait => 1,
            Orientation::Landscape => 6,
        };
        let app = cryon_client_input::android::app()
            .ok_or_else(|| "the system never gave the app its activity".to_owned())?;
        // SAFETY: the pointer is the JavaVM the system handed this process,
        // published by android-activity precisely so an app can reach it.
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            // SAFETY: the pointer above is this activity's own instance, and the
            // reference is dropped when the closure ends.
            let activity = unsafe { jni::objects::JObject::from_raw(&*env, activity) };
            let takes_int = jni::signature::RuntimeMethodSignature::from_str("(I)V")?;
            env.call_method(
                &activity,
                jni::strings::JNIString::from("setRequestedOrientation"),
                takes_int.method_signature(),
                &[jni::objects::JValue::Int(value)],
            )?;

            let gets_window = jni::signature::RuntimeMethodSignature::from_str("()Landroid/view/Window;")?;
            let window = env
                .call_method(
                    &activity,
                    jni::strings::JNIString::from("getWindow"),
                    gets_window.method_signature(),
                    &[],
                )?
                .l()?;
            let gets_controller =
                jni::signature::RuntimeMethodSignature::from_str("()Landroid/view/WindowInsetsController;")?;
            let controller = env
                .call_method(
                    &window,
                    jni::strings::JNIString::from("getInsetsController"),
                    gets_controller.method_signature(),
                    &[],
                )?
                .l()?;
            // Null before the window is attached to a display; the next call,
            // which comes with a real window, sets it.
            if !controller.is_null() {
                env.call_method(
                    &controller,
                    jni::strings::JNIString::from("hide"),
                    takes_int.method_signature(),
                    &[jni::objects::JValue::Int(NAVIGATION_BARS)],
                )?;
                env.call_method(
                    &controller,
                    jni::strings::JNIString::from("setSystemBarsBehavior"),
                    takes_int.method_signature(),
                    &[jni::objects::JValue::Int(SHOW_ON_SWIPE)],
                )?;
            }
            ask_about_the_camera(env, &window)?;
            Ok(())
        })
        .map_err(|error| format!("the activity would not take the screen: {error}"))
    });
}

#[cfg(not(target_os = "android"))]
fn install_orientation_handler() {}

/// The behaviour runtime's bridge: the engine's tick reaching Java.
///
/// `cryon_client::game::bridge::set_tick_handler` takes one plain function;
/// this module hands it [`tick`], which calls the static
/// `dev.cryon.client.fabricapi.Bridge.tick` in the app, where the shim fans
/// the call out to every listener an imported mod registered. The class is
/// resolved once, here on the app's own thread, through the activity's class
/// loader — a thread attached later cannot `FindClass` an app class, so the
/// global reference is the whole trick.
///
/// Ticks arrive from the window's game and the server's own, on whichever
/// threads run them; `attach_current_thread` makes each callable, and the
/// call itself is the cheap kind that JNI already guards.
#[cfg(target_os = "android")]
mod behavior {
    use std::sync::OnceLock;

    static BRIDGE: OnceLock<jni::refs::GlobalRef<jni::objects::JObject<'static>>> =
        OnceLock::new();

    /// Resolves the bridge class and registers [`tick`]. Never fails the game:
    /// without the bridge the behaviour runtime simply hears no ticks, which
    /// is what a launcher without the shim's Java installed would mean anyway.
    pub fn install() {
        let Some(app) = cryon_client_input::android::app() else {
            log::warn!("Cryon: behaviour bridge: no activity to resolve through");
            return;
        };
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
        let outcome = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            // SAFETY: the pointer above is this activity's own instance,
            // and the reference is dropped when the closure ends.
            let activity = unsafe { jni::objects::JObject::from_raw(&*env, activity) };
            let gets_loader = jni::signature::RuntimeMethodSignature::from_str(
                "()Ljava/lang/ClassLoader;",
            )?;
            let loader = env
                .call_method(
                    &activity,
                    jni::strings::JNIString::from("getClassLoader"),
                    gets_loader.method_signature(),
                    &[],
                )?
                .l()?;
            let loads = jni::signature::RuntimeMethodSignature::from_str(
                "(Ljava/lang/String;)Ljava/lang/Class;",
            )?;
            let name = jni::objects::JString::from_str(
                env,
                "dev.cryon.client.fabricapi.Bridge",
            )?;
            let class = env
                .call_method(
                    &loader,
                    jni::strings::JNIString::from("loadClass"),
                    loads.method_signature(),
                    &[jni::objects::JValue::Object(&name)],
                )?
                .l()?;
            let _ = BRIDGE.set(env.new_global_ref(&class)?);
            Ok(())
        });
        if outcome.is_ok() && BRIDGE.get().is_some() {
            cryon_client::game::bridge::set_tick_handler(tick);
            log::info!("Cryon: behaviour bridge installed");
        } else {
            log::warn!("Cryon: behaviour bridge not installed: {outcome:?}");
        }
    }

    /// The function the engine calls once per tick. Quietly does nothing
    /// before [`install`] has resolved the class, and a JNI failure costs
    /// this tick only — a mod's listener failing must never stop the world.
    /// The first call and the first failure each speak to logcat once, so a
    /// silent bridge can be told apart from a broken one.
    fn tick(ticks: i64) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static FIRST_CALL: AtomicBool = AtomicBool::new(false);
        static FIRST_FAILURE: AtomicBool = AtomicBool::new(false);
        if !FIRST_CALL.swap(true, Ordering::Relaxed) {
            log::info!("Cryon: behaviour bridge first tick {ticks}");
        }
        let Some(class) = BRIDGE.get() else { return };
        let Some(app) = cryon_client_input::android::app() else { return };
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
        let outcome = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let takes_int =
                jni::signature::RuntimeMethodSignature::from_str("(I)V")?;
            // The stored global is a plain JObject; `as_cast` is the crate's
            // sanctioned, runtime-checked way to view it as a JClass.
            let class = env.as_cast::<jni::objects::JClass>(class.as_obj())?;
            env.call_static_method(
                class.as_ref(),
                jni::strings::JNIString::from("tick"),
                takes_int.method_signature(),
                &[jni::objects::JValue::Int(ticks as i32)],
            )?;
            Ok(())
        });
        if let Err(failure) = outcome {
            if !FIRST_FAILURE.swap(true, Ordering::Relaxed) {
                log::warn!("Cryon: behaviour bridge tick failed: {failure:?}");
            }
        }
    }
}

///
/// Where the front camera goes: the game drawn under it, or stopping short of it.
///
/// The theme asks for the cutout, which is what fills the screen edge to edge on a
/// phone with a punch-hole; `View.setSystemUiVisibility`-style tricks are ignored
/// from Android 11 on, so this is a window attribute. Some players would rather the
/// picture stopped before the camera, and that is the same attribute set the other
/// way — a window change, not a renderer change — so the launcher's choice is read
/// from `CRYON_CUTOUT` and applied here. Nothing is touched when the switch is
/// absent, which leaves the theme in charge.
#[cfg(target_os = "android")]
fn ask_about_the_camera(
    env: &mut jni::Env<'_>,
    window: &jni::objects::JObject<'_>,
) -> jni::errors::Result<()> {
    // WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES (1)
    // and _NEVER (2).
    let mode = match std::env::var("CRYON_CUTOUT").unwrap_or_default().trim() {
        "fill" => 1,
        "inset" => 2,
        _ => return Ok(()),
    };
    let gets_attrs = jni::signature::RuntimeMethodSignature::from_str(
        "()Landroid/view/WindowManager$LayoutParams;",
    )?;
    let params = env
        .call_method(
            window,
            jni::strings::JNIString::from("getAttributes"),
            gets_attrs.method_signature(),
            &[],
        )?
        .l()?;
    if params.is_null() {
        return Ok(());
    }
    let int_field = jni::signature::RuntimeFieldSignature::from_str("I")?;
    env.set_field(
        &params,
        jni::strings::JNIString::from("layoutInDisplayCutoutMode"),
        int_field.field_signature(),
        jni::objects::JValue::Int(mode),
    )?;
    let sets_attrs = jni::signature::RuntimeMethodSignature::from_str(
        "(Landroid/view/WindowManager$LayoutParams;)V",
    )?;
    env.call_method(
        window,
        jni::strings::JNIString::from("setAttributes"),
        sets_attrs.method_signature(),
        &[jni::objects::JValue::Object(&params)],
    )?;
    Ok(())
}

/// Where the prepared resources and the player's files live on this device.
///
/// Shared storage (`Android/data/<package>/files`) rather than the private data
/// dir because a phone's resources have to be copyable onto it without root, and
/// it is the same folder a file manager and a mobile launcher can see.
#[cfg(target_os = "android")]
fn point_at_app_dirs(app: &android_activity::AndroidApp) {
    let files = app
        .external_data_path()
        .or_else(|| app.internal_data_path().map(|d| d.join("files")))
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/cryon"));

    let resources = files.join("resources");
    let game_dir = files.join("game");
    if let Err(error) = std::fs::create_dir_all(&game_dir) {
        log::warn!("Cryon: cannot create {game_dir:?}: {error}");
    }

    // Set in this process only, before any thread reads the environment.
    unsafe {
        std::env::set_var("CRYON_RESOURCES", &resources);
        std::env::set_var("CRYON_GAME_DIR", &game_dir);
        std::env::set_var("CRYON_HOME", &game_dir);
        // The drawer in the game writes a switch back when a player changes one,
        // so it needs the file's own name rather than the folder it sits in.
        std::env::set_var("CRYON_ENV_FILE", files.join("env.txt"));
    }
    let switches = apply_switches(&files);
    // Sound is off on a phone until the mixer stops sharing one mutex with the
    // audio callback: at 44.1 kHz and a low-latency buffer that callback runs
    // about 460 times a second, and every `play` from the game thread waits on
    // it. Measured on a Redmi Note 14 Pro: 1100 ms a frame with sound, 16.8 ms
    // without, 7 fps against 59. `CRYON_AUDIO=1` in env.txt asks for sound back.
    if !matches!(std::env::var("CRYON_AUDIO").as_deref(), Ok("1" | "true")) {
        // SAFETY: still before the game starts any thread of its own.
        unsafe { std::env::set_var("CRYON_NO_AUDIO", "1") };
    }
    log::info!(
        "Cryon: resources {} ({} entries), game {}{}{}",
        resources.display(),
        std::fs::read_dir(&resources).map_or(0, |d| d.count()),
        game_dir.display(),
        if switches.is_empty() { "" } else { ", switches: " },
        switches.join(", ")
    );
}

/// Applies `files/env.txt` to the environment and returns what it set.
///
/// A phone gives an app no command line and no way to set a variable, and the
/// engine's diagnostics and fallbacks are switched by both: `CRYON_SPIKES=40`
/// logs every frame slower than 40 ms with the milliseconds each part of it
/// took, `CRYON_NO_AUDIO=1` runs the game silent. So one file, one line per
/// `KEY=VALUE`, read before the game starts: push it, restart, read logcat.
/// Delete it and the game is ordinary again.
fn apply_switches(files: &std::path::Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(files.join("env.txt")) else {
        return Vec::new();
    };
    let mut set = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            log::warn!("Cryon: env.txt: ignoring {line} (not KEY=VALUE)");
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key.is_empty() || key.contains(['\0', '=']) || value.contains('\0')
            || !key.starts_with("CRYON_") {
            log::warn!("Cryon: env.txt: ignoring an invalid switch");
            continue;
        }
        // SAFETY: this runs before the game starts any thread, so nothing else
        // is reading the environment yet.
        unsafe { std::env::set_var(key, value) };
        set.push(key.to_owned());
    }
    set
}

#[cfg(not(target_os = "android"))]
fn point_at_app_dirs(_app: &android_activity::AndroidApp) {}

/// Shared offline importer, called on a worker in Android's separate :porter process.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_cryon_client_ModsActivity_importNative<'caller>(
    mut unowned: jni::EnvUnowned<'caller>,
    _class: jni::objects::JClass<'caller>,
    jar: jni::objects::JString<'caller>,
    work: jni::objects::JString<'caller>,
    game: jni::objects::JString<'caller>,
    resources: jni::objects::JString<'caller>,
) -> jni::objects::JString<'caller> {
    unowned.with_env(|env| -> Result<_, jni::errors::Error> {
        let (jar, work, game, resources) = (jar.to_string(), work.to_string(), game.to_string(), resources.to_string());
        let result = cryon_port_pack::check::import_one(
            std::path::Path::new(&jar), std::path::Path::new(&work),
            std::path::Path::new(&game),
            (!resources.is_empty()).then(|| std::path::Path::new(&resources)),
        );
        let json = match result {
            Ok(report) => serde_json::to_string(&report).unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}).to_string()),
            Err(error) => serde_json::json!({"error": error}).to_string(),
        };
        jni::objects::JString::from_str(env, json)
    }).resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// `log`'s output into logcat, which is the only place a phone will show it.
///
/// `__android_log_write` rather than `__android_log_print`: the print form takes
/// varargs, and calling those from Rust is undefined behaviour unless the format
/// string is a literal. The write form takes one plain string.
struct Logcat;

impl log::Log for Logcat {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    #[cfg(target_os = "android")]
    fn log(&self, record: &log::Record) {
        const VERBOSE: i32 = 2;
        const DEBUG: i32 = 3;
        const INFO: i32 = 4;
        const WARN: i32 = 5;
        const ERROR: i32 = 6;
        let prio = match record.level() {
            log::Level::Error => ERROR,
            log::Level::Warn => WARN,
            log::Level::Info => INFO,
            log::Level::Debug => DEBUG,
            log::Level::Trace => VERBOSE,
        };
        // A NUL anywhere in the string would truncate it, and one in the middle
        // is worth keeping rather than dropping the whole line for.
        let text = format!("{}: {}", record.target(), record.args()).replace('\0', " ");
        let tag = std::ffi::CString::new("Cryon").expect("no nul in the tag");
        let message = std::ffi::CString::new(text).expect("nul replaced above");
        // SAFETY: both pointers are alive for the call, and the C function only
        // reads them.
        unsafe {
            __android_log_write(prio, tag.as_ptr(), message.as_ptr());
        }
    }

    #[cfg(not(target_os = "android"))]
    fn log(&self, record: &log::Record) {
        eprintln!("{}: {}", record.level(), record.args());
    }

    fn flush(&self) {}
}

#[cfg(target_os = "android")]
unsafe extern "C" {
    // c_char, not i8: it is unsigned on arm64 and signed on x86_64, so naming
    // the width here would only build for one of the two.
    fn __android_log_write(prio: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
}
