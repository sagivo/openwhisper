//! First-run permission prompts (microphone + macOS Accessibility + Input Monitoring).

/// Whether this process is trusted for Accessibility (keystroke injection).
/// Non-macOS always returns true — there is no equivalent TCC gate.
pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::accessibility_trusted()
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
        macos::prompt_accessibility()
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
        macos::open_pane("Privacy_Accessibility");
    }
}

/// Whether this process may listen to keyboard events (Fn, global taps).
/// Non-macOS always returns true.
pub fn input_monitoring_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::input_monitoring_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Show the system Input Monitoring prompt if access is not already granted.
pub fn prompt_input_monitoring() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::prompt_input_monitoring()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Open the OS pane where Input Monitoring can be granted.
pub fn open_input_monitoring_settings() {
    #[cfg(target_os = "macos")]
    {
        macos::open_pane("Privacy_ListenEvent");
    }
}

pub fn hotkey_needs_listen_event(hotkey: &str) -> bool {
    hotkey.split('+').any(|part| {
        matches!(
            part.trim().to_ascii_lowercase().as_str(),
            "fn" | "function"
        )
    })
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

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOHIDCheckAccess(request_type: u32) -> i32;
        fn IOHIDRequestAccess(request_type: u32) -> bool;
    }

    const IOHID_REQUEST_LISTEN_EVENT: u32 = 1;
    const IOHID_ACCESS_GRANTED: i32 = 0;

    pub fn accessibility_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn prompt_accessibility() -> bool {
        if accessibility_trusted() {
            return true;
        }
        unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let val = CFBoolean::true_value();
            let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
            AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef())
        }
    }

    pub fn input_monitoring_trusted() -> bool {
        unsafe { IOHIDCheckAccess(IOHID_REQUEST_LISTEN_EVENT) == IOHID_ACCESS_GRANTED }
    }

    pub fn prompt_input_monitoring() -> bool {
        if input_monitoring_trusted() {
            return true;
        }
        unsafe { IOHIDRequestAccess(IOHID_REQUEST_LISTEN_EVENT) }
    }

    pub fn open_pane(privacy_key: &str) {
        let url = format!(
            "x-apple.systempreferences:com.apple.preference.security?{privacy_key}"
        );
        let _ = Command::new("open").arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fn_hotkey_needs_listen_event() {
        assert!(hotkey_needs_listen_event("Fn"));
        assert!(hotkey_needs_listen_event("fn"));
        assert!(hotkey_needs_listen_event("function"));
        assert!(hotkey_needs_listen_event("Shift+Fn"));
        assert!(!hotkey_needs_listen_event("Cmd+Shift+Space"));
        assert!(!hotkey_needs_listen_event("Ctrl+a"));
    }

    #[test]
    fn permission_checks_do_not_panic() {
        let _ = accessibility_trusted();
        let _ = input_monitoring_trusted();
    }

    #[test]
    fn non_macos_permissions_are_granted() {
        #[cfg(not(target_os = "macos"))]
        {
            assert!(accessibility_trusted());
            assert!(prompt_accessibility());
            assert!(input_monitoring_trusted());
            assert!(prompt_input_monitoring());
        }
    }
}
