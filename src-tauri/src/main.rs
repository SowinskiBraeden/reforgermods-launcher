// Release builds must not open a console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Works around a WebKitGTK startup crash on Nvidia under Wayland.
///
/// WebKitGTK's DMABUF renderer and the proprietary Nvidia driver disagree about
/// buffer import, and GTK aborts before the window is ever mapped:
///
/// ```text
/// Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.
/// ```
///
/// Setting `WEBKIT_DISABLE_DMABUF_RENDERER=1` falls back to a software
/// composite path, which costs some GPU acceleration and produces a window that
/// actually appears. Reported against Arch with an Nvidia card; it is a WebKit
/// and driver interaction rather than anything this app controls.
///
/// Deliberately narrow, because the fallback is not free for anyone who does not
/// need it:
///
/// - an existing value is never overridden, so a user debugging this keeps control;
/// - only when the Nvidia driver is actually loaded;
/// - only in a Wayland session, since the X11 path does not use this renderer;
/// - not when GDK is already pinned to X11, where it is equally unused.
#[cfg(target_os = "linux")]
fn disable_dmabuf_renderer_on_nvidia_wayland() {
    let decision = should_disable_dmabuf(
        std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("GDK_BACKEND")
            .as_deref()
            .and_then(|b| b.to_str()),
        std::path::Path::new("/proc/driver/nvidia/version").exists()
            || std::path::Path::new("/sys/module/nvidia").exists(),
    );
    if !decision {
        return;
    }
    // SAFETY: this runs at the top of main, before Tauri, GTK or any thread
    // exists, so there is no concurrent reader of the environment. It must also
    // happen here rather than later: WebKit reads this once, during init.
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
}

/// The decision behind [`disable_dmabuf_renderer_on_nvidia_wayland`], separated
/// from the environment so each condition can be tested.
#[cfg(target_os = "linux")]
fn should_disable_dmabuf(
    already_set: bool,
    wayland_session: bool,
    gdk_backend: Option<&str>,
    nvidia_loaded: bool,
) -> bool {
    if already_set || !wayland_session || !nvidia_loaded {
        return false;
    }
    // A GDK_BACKEND naming x11 alone means the Wayland renderer is not in play.
    // A list such as "wayland,x11" still prefers Wayland, so only the exact pin
    // counts as opting out.
    gdk_backend != Some("x11")
}

fn main() {
    #[cfg(target_os = "linux")]
    disable_dmabuf_renderer_on_nvidia_wayland();

    reforgermods_launcher_lib::run()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::should_disable_dmabuf;

    #[test]
    fn nvidia_under_wayland_is_the_case_that_needs_the_workaround() {
        assert!(should_disable_dmabuf(false, true, None, true));
        assert!(should_disable_dmabuf(false, true, Some("wayland"), true));
        // Still Wayland-first, so the renderer is still the one that breaks.
        assert!(should_disable_dmabuf(
            false,
            true,
            Some("wayland,x11"),
            true
        ));
    }

    #[test]
    fn an_explicit_setting_is_never_overridden() {
        // Someone debugging this has set it to 0 on purpose; honour that.
        assert!(!should_disable_dmabuf(true, true, None, true));
    }

    #[test]
    fn other_sessions_and_drivers_keep_accelerated_rendering() {
        assert!(
            !should_disable_dmabuf(false, false, None, true),
            "X11 session"
        );
        assert!(
            !should_disable_dmabuf(false, true, None, false),
            "not Nvidia"
        );
        assert!(
            !should_disable_dmabuf(false, true, Some("x11"), true),
            "pinned to XWayland"
        );
    }
}
