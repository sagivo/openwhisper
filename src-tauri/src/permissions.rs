//! First-run permission prompts (microphone + macOS Accessibility).

/// Whether this process is trusted for Accessibility (keystroke injection).
/// Non-macOS always returns true — there is no equivalent TCC gate.
pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Show the system Accessibility prompt if we are not already trusted.
/// Returns the current trusted state (may still be false until the user toggles).
pub fn prompt_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::prompt()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Open the OS pane where Accessibility can be granted.
pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        macos::open_settings();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation_sys::dictionary::CFDictionaryRef;
    use std::process::Command;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn prompt() -> bool {
        if is_trusted() {
            return true;
        }
        unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let val = CFBoolean::true_value();
            let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
            AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef())
        }
    }

    pub fn open_settings() {
        // Works on both System Preferences (Monterey) and System Settings.
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}
