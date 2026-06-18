//! Direct injection-path test. Bypasses Whisper/Gemma entirely and just
//! exercises the clipboard + Cmd+V path that lib::run_pipeline ends with.
//!
//! Usage:
//!   cargo run --example inject_test --features metal -- "hello from openwhisper"
//!
//! It will:
//!   1. snapshot the current clipboard
//!   2. set the clipboard to the provided text
//!   3. query macOS Accessibility for the focused element role
//!   4. send Cmd+V
//!   5. wait 250 ms, restore the snapshotted clipboard
//!
//! Watch the terminal output AND the focused window to see if the text
//! arrives. If the text doesn't appear in your focused field but the log
//! says "focused_editable=Some(true)" and "send_paste OK", the failure is
//! between enigo and the OS event tap (usually Accessibility permissions
//! on the *terminal* running this binary, since macOS attributes the
//! synthesized keystrokes to the calling process).

use anyhow::Result;
use openwhisper_lib::injector;
use std::time::Duration;

fn main() -> Result<()> {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hello from openwhisper inject_test".to_string());

    println!("== inject_test ==");
    println!("Text to inject: {text:?}");
    println!();
    println!("You have 4 seconds to focus the target app/text field. Switch now.");
    for i in (1..=4).rev() {
        println!("  {i}…");
        std::thread::sleep(Duration::from_secs(1));
    }
    println!();

    println!("Snapshotting clipboard…");
    let original = injector::snapshot_clipboard();
    println!("  prior clipboard: {:?}", original.as_deref().map(short));

    println!("Calling injector::paste_sync…");
    let n = match injector::paste_sync(&text) {
        Ok(n) => {
            println!("  paste_sync returned {n} units");
            n
        }
        Err(e) => {
            println!("  paste_sync ERROR: {e}");
            return Err(e);
        }
    };

    println!("Waiting 500 ms for paste to land…");
    std::thread::sleep(Duration::from_millis(500));

    if let Some(prev) = original {
        println!("Restoring clipboard asynchronously…");
        injector::restore_clipboard_async(prev);
        std::thread::sleep(Duration::from_millis(400));
    }

    println!();
    println!("Done. Pasted {n} UTF-16 units.");
    println!("If nothing appeared in your focused field:");
    println!("  - Check System Settings → Privacy & Security → Accessibility,");
    println!("    and ensure your terminal (Terminal.app / iTerm / etc.) is enabled.");
    println!("  - Try focusing a known-working text field (a TextEdit document).");
    Ok(())
}

fn short(s: &str) -> String {
    if s.len() > 40 {
        format!("{}…", &s[..40])
    } else {
        s.to_string()
    }
}
