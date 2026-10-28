//! The system's display accessibility options the UI follows (`docs/plans/glass.md`, G12).
//!
//! "Reduce transparency" turns the app's glass dense, as "Reduce motion" turns its movement off. The webview's
//! `prefers-reduced-transparency` cannot be relied on in WebKit, so the setting is read here from `NSWorkspace`. The
//! class is looked up by name rather than linked: outside an app that loaded AppKit (a plain `cargo test`) there is
//! none, and the answer is then simply "no".

/// Whether macOS asks apps to reduce transparency (System Settings → Accessibility → Display).
#[cfg(target_os = "macos")]
pub fn reduces_transparency() -> bool {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool};

    let Some(class) = AnyClass::get(c"NSWorkspace") else {
        return false;
    };
    // A pool of its own: the call runs on a blocking worker thread, which has none, and AppKit may autorelease inside.
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: `sharedWorkspace` returns the process's one workspace object (or nil, checked), and
        // `accessibilityDisplayShouldReduceTransparency` is a plain BOOL property getter on it. Apple does not document
        // it as main-thread-only, and reading a BOOL off another thread is safe in practice; a stale answer is
        // corrected at the next focus.
        unsafe {
            let workspace: *mut AnyObject = msg_send![class, sharedWorkspace];
            if workspace.is_null() {
                return false;
            }
            let reduces: Bool = msg_send![workspace, accessibilityDisplayShouldReduceTransparency];
            reduces.as_bool()
        }
    })
}

/// Elsewhere there is no such setting to follow.
#[cfg(not(target_os = "macos"))]
pub fn reduces_transparency() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_no_without_appkit_loaded() {
        // A test binary links no AppKit, so there is no NSWorkspace class: the lookup falls back to "no".
        assert!(!reduces_transparency());
    }
}
