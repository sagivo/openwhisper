use anyhow::{Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

/// Place `text` on the system clipboard and, if the focused UI element looks
/// like a text input, also send the platform paste shortcut so it lands in the
/// app the user was just typing into.
///
/// If we can't tell whether a text input is focused (or the focused element
/// isn't editable), we just leave the text on the clipboard so the user can
/// paste it manually.
///
/// Runs on a dedicated OS thread so we don't block the pipeline.
pub fn type_text(text: String, restore_clipboard: bool) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    std::thread::spawn(move || {
        if let Err(e) = inject(text, restore_clipboard) {
            log::error!("inject failed: {e}");
        }
    });
    Ok(())
}

/// Snapshot the current clipboard so the caller can restore it later (used
/// by the fast-paste flow which performs two pastes in quick succession and
/// has to manage the restore manually to avoid races).
pub fn snapshot_clipboard() -> Option<String> {
    read_clipboard().ok()
}

/// Restore the clipboard contents to `prev` after a short delay, on a
/// background thread. The delay lets target apps actually consume the most
/// recent paste before we overwrite it.
pub fn restore_clipboard_async(prev: String) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if let Err(e) = set_clipboard(&prev) {
            log::warn!("failed to restore clipboard: {e}");
        }
    });
}

/// Paste `text` synchronously and return how many UTF-16 code units it
/// occupies (the unit most editors use when counting backspaces). Used by the
/// fast-paste flow which inserts the raw transcript immediately and later
/// replaces it with the refined version.
///
/// Does NOT restore the clipboard — the caller is expected to manage that
/// using `snapshot_clipboard` / `restore_clipboard_async` so it can survive
/// multiple back-to-back paste operations without racing itself.
pub fn paste_sync(text: &str) -> Result<usize> {
    if text.is_empty() {
        return Ok(0);
    }
    inject(text.to_string(), /* restore_clipboard */ false)?;
    Ok(text.encode_utf16().count())
}

/// Replace the previously-pasted text with `new_text`. Sends `prev_units`
/// backspaces, then pastes the new text. Caller is responsible for deciding
/// when this is safe (i.e. shortly after the original paste, before the user
/// has had time to type). Also does NOT restore the clipboard — caller owns
/// the restore in the fast-paste flow.
pub fn replace_text(prev_units: usize, new_text: String) -> Result<()> {
    std::thread::spawn(move || {
        if let Err(e) = do_replace(prev_units, &new_text) {
            log::error!("replace failed: {e}");
        }
    });
    Ok(())
}

fn do_replace(prev_units: usize, new_text: &str) -> Result<()> {
    // Confirm a text field is still focused before we send a flurry of
    // backspaces. If detection is unavailable we err on the side of NOT
    // mutilating whatever the user's looking at.
    if focused_is_editable() == Some(false) {
        log::info!("replace skipped: focused element is no longer editable");
        return Ok(());
    }

    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("enigo init: {e}"))?;
    for _ in 0..prev_units {
        enigo
            .key(Key::Backspace, Direction::Click)
            .map_err(|e| anyhow::anyhow!("backspace: {e}"))?;
    }
    drop(enigo);

    inject(new_text.to_string(), /* restore_clipboard */ false)?;
    Ok(())
}

fn inject(text: String, restore_clipboard: bool) -> Result<()> {
    // Open a single clipboard handle for the read + write below. The previous
    // implementation called `read_clipboard()` and `set_clipboard()` separately,
    // each constructing their own `arboard::Clipboard` (two OS-level handles).
    let mut cb = arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard: {e}"))?;

    // Snapshot whatever the user had on the clipboard so we can put it back
    // after the paste lands. Many users keep important things on their
    // clipboard (passwords, URLs, code) and silently overwriting them is the
    // single biggest complaint about this class of app.
    let prior = if restore_clipboard {
        cb.get_text().ok()
    } else {
        None
    };

    cb.set_text(text.to_owned())
        .map_err(|e| anyhow::anyhow!("clipboard set: {e}"))?;
    // Some apps poll the clipboard; give them a moment before we trigger paste.
    std::thread::sleep(std::time::Duration::from_millis(40));

    let editable = focused_is_editable();
    log::info!("inject: focused_editable={editable:?}");

    // If detection failed (None) or we know it's editable, send paste.
    // If detection said "definitely not a text field", skip paste and just
    // leave the text on the clipboard.
    let should_paste = editable.unwrap_or(true);
    if should_paste {
        send_paste().with_context(|| "sending paste shortcut")?;

        // Restore the prior clipboard contents asynchronously so the rest of
        // the pipeline (refinement, follow-up replace, etc.) isn't blocked on
        // the restore delay. 250 ms is generous enough for sluggish Electron
        // apps (Slack, VS Code) to actually consume the clipboard but short
        // enough that the user is unlikely to Cmd+V again before we're done.
        if let Some(prev) = prior {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(250));
                if let Err(e) = set_clipboard(&prev) {
                    log::warn!("failed to restore clipboard: {e}");
                }
            });
        }
    }
    // If we did NOT paste, leave our text on the clipboard so the user can
    // paste it manually — restoring would defeat the whole point.
    Ok(())
}

fn read_clipboard() -> Result<String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard: {e}"))?;
    cb.get_text()
        .map_err(|e| anyhow::anyhow!("clipboard get: {e}"))
}

fn set_clipboard(text: &str) -> Result<()> {
    let mut cb = arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard: {e}"))?;
    cb.set_text(text.to_owned())
        .map_err(|e| anyhow::anyhow!("clipboard set: {e}"))?;
    // Some apps poll the clipboard; give them a moment before we trigger paste.
    std::thread::sleep(std::time::Duration::from_millis(40));
    Ok(())
}

fn send_paste() -> Result<()> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("enigo init: {e}"))?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    // IMPORTANT: do NOT use `Key::Unicode('v')` on macOS. Enigo translates
    // Unicode chars to keycodes via TSM (`TSMCurrentKeyboardInputSourceRefCreate`),
    // which on macOS 14+ asserts it is running on the main thread and
    // SIGTRAPs the process when called from a worker thread. We always
    // dispatch paste from a background thread, so we feed enigo the raw
    // virtual keycode for `V` (`kVK_ANSI_V = 0x09`) instead, which bypasses
    // TSM entirely. On other platforms we keep the portable Unicode path.
    #[cfg(target_os = "macos")]
    let v_key = Key::Other(0x09);
    #[cfg(not(target_os = "macos"))]
    let v_key = Key::Unicode('v');

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| anyhow::anyhow!("press mod: {e}"))?;
    enigo
        .key(v_key, Direction::Click)
        .map_err(|e| anyhow::anyhow!("press v: {e}"))?;
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| anyhow::anyhow!("release mod: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Focused-element detection
// ---------------------------------------------------------------------------

/// Returns `Some(true)` if the focused UI element accepts text input,
/// `Some(false)` if we're confident it doesn't, and `None` if we can't tell
/// (e.g. accessibility permissions missing, or non-macOS without detection).
fn focused_is_editable() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        macos::focused_is_editable()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation_sys::base::{CFRelease, CFTypeRef};
    use std::ffi::c_void;

    type AXUIElementRef = *mut c_void;
    type AXError = i32;
    const KAXERROR_SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementIsAttributeSettable(
            element: AXUIElementRef,
            attribute: CFStringRef,
            settable: *mut bool,
        ) -> AXError;
        fn AXIsProcessTrusted() -> bool;
    }

    pub fn focused_is_editable() -> Option<bool> {
        unsafe {
            if !AXIsProcessTrusted() {
                log::warn!(
                    "Accessibility permission not granted; cannot detect focused input. \
                     Grant it in System Settings → Privacy & Security → Accessibility."
                );
                return None;
            }

            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }

            let focused = match copy_attr(system, "AXFocusedUIElement") {
                Some(v) => v as AXUIElementRef,
                None => {
                    CFRelease(system as CFTypeRef);
                    // No reported focused element: don't claim non-editable.
                    // That used to make us swallow legitimate pastes when AX
                    // momentarily reported nothing during focus transitions.
                    log::info!("inject: no AXFocusedUIElement; will paste anyway");
                    return None;
                }
            };
            CFRelease(system as CFTypeRef);

            let role = copy_attr_string(focused, "AXRole");
            let subrole = copy_attr_string(focused, "AXSubrole");
            log::info!("focused role={role:?} subrole={subrole:?}");

            // Strict deny-list. We only refuse to paste when the focused
            // element is *unambiguously* a non-text-receiver — buttons,
            // menus, images, links, the menu bar itself. Anything else
            // (including unknown roles, web/Electron custom roles, and
            // anything where AX is being weird) gets a Cmd+V. A missed
            // paste is far worse for our use case than the occasional
            // macOS "funk" beep when the focused app rejects the paste.
            let definitely_not_editable = matches!(
                role.as_deref(),
                Some("AXButton")
                    | Some("AXImage")
                    | Some("AXMenuItem")
                    | Some("AXMenuBarItem")
                    | Some("AXMenu")
                    | Some("AXMenuBar")
                    | Some("AXCheckBox")
                    | Some("AXRadioButton")
                    | Some("AXLink")
                    | Some("AXStaticText")
                    | Some("AXSlider")
                    | Some("AXScrollBar")
            );

            CFRelease(focused as CFTypeRef);
            if definitely_not_editable {
                Some(false)
            } else {
                Some(true)
            }
        }
    }

    /// Currently unused — kept around because we may want to consult it
    /// again as a tie-breaker when the role-based deny-list above proves
    /// too permissive in some app. Returns whether the focused element's
    /// `AXValue` is settable, which is the canonical "this accepts text"
    /// signal in the AX API.
    #[allow(dead_code)]
    unsafe fn ax_value_settable(element: AXUIElementRef) -> Option<bool> {
        if element.is_null() {
            return None;
        }
        let attr = CFString::new("AXValue");
        let mut settable = false;
        let err =
            AXUIElementIsAttributeSettable(element, attr.as_concrete_TypeRef(), &mut settable);
        if err == KAXERROR_SUCCESS {
            Some(settable)
        } else {
            None
        }
    }

    unsafe fn copy_attr(element: AXUIElementRef, attr: &str) -> Option<CFTypeRef> {
        if element.is_null() {
            return None;
        }
        let attr = CFString::new(attr);
        let mut out: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut out);
        if err == KAXERROR_SUCCESS && !out.is_null() {
            Some(out)
        } else {
            None
        }
    }

    unsafe fn copy_attr_string(element: AXUIElementRef, attr: &str) -> Option<String> {
        let v = copy_attr(element, attr)?;
        let s = CFString::wrap_under_create_rule(v as CFStringRef);
        Some(s.to_string())
    }

    // Silence dead-code warning if CFType import is otherwise unused.
    #[allow(dead_code)]
    fn _force_link(_t: CFType) {}
}
