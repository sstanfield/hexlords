//! What the window needs from the operating system that miniquad does not do.
//!
//! This is the one module that may hold `unsafe`: a call into C or
//! Objective-C, behind a safe function, as small as the call, with a `SAFETY`
//! comment that says why it is sound.

/// Bring the game's window in front of the other applications' windows. Call it
/// from one of miniquad's callbacks (they run on the main thread, once the
/// window exists).
///
/// On macOS a program started from a terminal opens its window *behind* the
/// terminal. miniquad 0.4.11 asks for activation in a delegate callback
/// (`applicationDidUpdate:`) with `NSRunningApplication activateWithOptions:`,
/// and on current macOS that request is not honoured; asking `NSApplication`
/// itself is. (Tried on macOS 15.8: miniquad alone and `[NSApp activate]` leave
/// the terminal in front, `[NSApp activateIgnoringOtherApps:YES]` brings the
/// window forward, and a helper process started by the game is refused.) So
/// this sends that one message.
///
/// It is the only `unsafe` in the crate, and it adds no dependency: the
/// Objective-C plumbing is what miniquad re-exports. If miniquad fixes its
/// activation, delete this. On other systems the window manager puts new
/// windows in front, and this does nothing.
pub fn bring_to_front() {
    #[cfg(target_os = "macos")]
    macos::activate();
}

#[cfg(target_os = "macos")]
mod macos {
    use miniquad::native::apple::frameworks::{ObjcId, YES, class, msg_send, sel, sel_impl};

    #[allow(unsafe_code)]
    pub fn activate() {
        // SAFETY: `NSApplication sharedApplication` returns the application object
        // (it exists: miniquad created it before any callback runs), and
        // `activateIgnoringOtherApps:` takes one BOOL and returns nothing. Both
        // are sent on the main thread, where miniquad runs its callbacks, as
        // AppKit requires.
        unsafe {
            let app: ObjcId = msg_send![class!(NSApplication), sharedApplication];
            let () = msg_send![app, activateIgnoringOtherApps: YES];
        }
    }
}
