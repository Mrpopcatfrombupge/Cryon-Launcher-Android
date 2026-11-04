//! Android host glue: the `AndroidApp` handle and the event loop that carries it.
//!
//! winit can only build a usable Android event loop if it is handed the
//! `AndroidApp` that the system passed to `android_main`. Desktop has no such
//! requirement, so the difference is kept here rather than at every call site.

use winit::event_loop::EventLoop;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
static APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();

/// Called once by `android_main` before the game starts.
#[cfg(target_os = "android")]
pub fn set_app(app: AndroidApp) {
    // A second call means the process was recreated; the first handle is the
    // one the running event loop is bound to, so it stays.
    let _ = APP.set(app);
}

#[cfg(target_os = "android")]
pub fn app() -> Option<&'static AndroidApp> {
    APP.get()
}

#[cfg(not(target_os = "android"))]
pub fn set_app(_app: ()) {}

/// Which way the screen should be held. The launcher is read top to bottom and
/// belongs in portrait; the game wants the wide view, so PLAY asks for landscape
/// and the phone is turned, not the app stretched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

type OrientationHandler = dyn Fn(Orientation) -> Result<(), String> + Send + Sync + 'static;

#[cfg(target_os = "android")]
static ORIENTATION: std::sync::OnceLock<Box<OrientationHandler>> = std::sync::OnceLock::new();

/// Hands over the function that actually turns the screen.
///
/// Rotating an activity is a Java call — `Activity.setRequestedOrientation` —
/// and there is no C or Rust entry point for it, so it needs JNI. This crate
/// forbids unsafe code, and the entry shim is the one place that already does
/// foreign calls, so the shim registers the closure here at startup and every
/// caller below stays safe.
#[cfg(target_os = "android")]
pub fn set_orientation_handler(handler: impl Fn(Orientation) -> Result<(), String> + Send + Sync + 'static) {
    let _ = ORIENTATION.set(Box::new(handler));
}

#[cfg(not(target_os = "android"))]
pub fn set_orientation_handler(_: impl Fn(Orientation) -> Result<(), String> + Send + Sync + 'static) {}

/// Asks the system to turn the screen. An error is a phone that refused, not a
/// reason to stop the game: the caller logs it and carries on at whatever angle
/// the window already has.
pub fn set_orientation(orientation: Orientation) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        ORIENTATION
            .get()
            .map(|handler| handler(orientation))
            .unwrap_or_else(|| Err("nothing registered to turn the screen".to_owned()))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = orientation;
        Err("only a phone turns its own screen".to_owned())
    }
}

/// An event loop for this platform. On Android this fails loudly if
/// `set_app` never ran, because a loop built without the handle cannot create
/// a window and the failure would otherwise surface much later as a dead window.
pub fn event_loop() -> Result<EventLoop<()>, winit::error::EventLoopError> {
    #[cfg(target_os = "android")]
    {
        use winit::platform::android::EventLoopBuilderExtAndroid;
        let Some(app) = APP.get() else {
            // No handle to attach. Let winit report the real failure rather
            // than inventing one: its error types are not constructible here.
            return EventLoop::builder().build();
        };
        EventLoop::builder().with_android_app(app.clone()).build()
    }
    #[cfg(not(target_os = "android"))]
    {
        EventLoop::new()
    }
}
