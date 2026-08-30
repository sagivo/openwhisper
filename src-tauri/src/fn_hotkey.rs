use anyhow::{anyhow, Result};
use std::sync::Arc;

type EdgeFn = Arc<dyn Fn(bool) + Send + Sync>;

pub fn start(on_edge: EdgeFn) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::start(on_edge)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on_edge;
        Err(anyhow!("Fn listener is only available on macOS"))
    }
}

pub fn stop() {
    #[cfg(target_os = "macos")]
    {
        macos::stop();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use parking_lot::Mutex;
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    const KEY_DOWN: u32 = 10;
    const KEY_UP: u32 = 11;
    const FLAGS_CHANGED: u32 = 12;
    const TAP_DISABLED_TIMEOUT: u32 = 0xFFFFFFFE;
    const TAP_DISABLED_USER: u32 = 0xFFFFFFFF;
    const SECONDARY_FN: u64 = 0x0080_0000;
    const KEYCODE_FN: i64 = 0x3F;
    const KEYBOARD_EVENT_KEYCODE: u32 = 9;

    #[repr(u32)]
    #[allow(dead_code)]
    enum TapLocation {
        Hid = 0,
        Session = 1,
    }

    #[repr(u32)]
    enum TapPlacement {
        HeadInsert = 0,
    }

    #[repr(u32)]
    enum TapOptions {
        ListenOnly = 1,
    }

    type MachPort = *mut c_void;
    type RunLoop = *mut c_void;
    type RunLoopSource = *mut c_void;
    type RunLoopMode = *const c_void;
    type Allocator = *mut c_void;
    type EventRef = *const c_void;

    type TapCallback = unsafe extern "C" fn(
        proxy: *const c_void,
        etype: u32,
        event: EventRef,
        user_info: *const c_void,
    ) -> EventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: TapLocation,
            place: TapPlacement,
            options: TapOptions,
            events_of_interest: u64,
            callback: TapCallback,
            user_info: *const c_void,
        ) -> MachPort;
        fn CGEventTapEnable(tap: MachPort, enable: bool);
        fn CGEventGetFlags(event: EventRef) -> u64;
        fn CGEventGetIntegerValueField(event: EventRef, field: u32) -> i64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopCommonModes: RunLoopMode;
        static kCFAllocatorDefault: Allocator;
        fn CFRunLoopGetCurrent() -> RunLoop;
        fn CFRunLoopRun();
        fn CFRunLoopStop(rl: RunLoop);
        fn CFMachPortCreateRunLoopSource(
            allocator: Allocator,
            port: MachPort,
            order: isize,
        ) -> RunLoopSource;
        fn CFMachPortInvalidate(port: MachPort);
        fn CFRunLoopAddSource(rl: RunLoop, source: RunLoopSource, mode: RunLoopMode);
        fn CFRunLoopRemoveSource(rl: RunLoop, source: RunLoopSource, mode: RunLoopMode);
        fn CFRelease(cf: *const c_void);
    }

    struct Tap {
        tap: MachPort,
        source: RunLoopSource,
        rl: RunLoop,
    }

    unsafe impl Send for Tap {}

    static ON_EDGE: Mutex<Option<EdgeFn>> = Mutex::new(None);
    static TAP: Mutex<Option<Tap>> = Mutex::new(None);
    static THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
    static LAST_FN: AtomicBool = AtomicBool::new(false);
    static TAP_PTR: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
    static RUNNING: AtomicBool = AtomicBool::new(false);

    fn event_mask() -> u64 {
        (1u64 << KEY_DOWN) | (1u64 << KEY_UP) | (1u64 << FLAGS_CHANGED)
    }

    unsafe fn create_tap() -> MachPort {
        let tap = CGEventTapCreate(
            TapLocation::Session,
            TapPlacement::HeadInsert,
            TapOptions::ListenOnly,
            event_mask(),
            callback,
            ptr::null(),
        );
        if !tap.is_null() {
            return tap;
        }
        CGEventTapCreate(
            TapLocation::Hid,
            TapPlacement::HeadInsert,
            TapOptions::ListenOnly,
            event_mask(),
            callback,
            ptr::null(),
        )
    }

    unsafe extern "C" fn callback(
        _proxy: *const c_void,
        etype: u32,
        event: EventRef,
        _user_info: *const c_void,
    ) -> EventRef {
        if etype == TAP_DISABLED_TIMEOUT || etype == TAP_DISABLED_USER {
            let tap = TAP_PTR.load(Ordering::SeqCst);
            if !tap.is_null() {
                CGEventTapEnable(tap, true);
            }
            return event;
        }

        let fn_now = match etype {
            FLAGS_CHANGED => CGEventGetFlags(event) & SECONDARY_FN != 0,
            KEY_DOWN | KEY_UP => {
                if CGEventGetIntegerValueField(event, KEYBOARD_EVENT_KEYCODE) != KEYCODE_FN {
                    return event;
                }
                etype == KEY_DOWN
            }
            _ => return event,
        };

        let was = LAST_FN.swap(fn_now, Ordering::SeqCst);
        if fn_now != was {
            if let Some(cb) = ON_EDGE.lock().clone() {
                cb(fn_now);
            }
        }
        event
    }

    pub fn start(on_edge: EdgeFn) -> Result<()> {
        stop();
        LAST_FN.store(false, Ordering::SeqCst);
        *ON_EDGE.lock() = Some(on_edge);

        let (tx, rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("ow-fn-tap".into())
            .spawn(move || unsafe {
                let tap = create_tap();
                if tap.is_null() {
                    let _ = tx.send(Err(()));
                    return;
                }
                let source =
                    CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
                if source.is_null() {
                    CFMachPortInvalidate(tap);
                    CFRelease(tap as *const c_void);
                    let _ = tx.send(Err(()));
                    return;
                }
                let rl = CFRunLoopGetCurrent();
                CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                TAP_PTR.store(tap, Ordering::SeqCst);
                *TAP.lock() = Some(Tap { tap, source, rl });
                RUNNING.store(true, Ordering::SeqCst);
                let _ = tx.send(Ok(()));
                CFRunLoopRun();
                RUNNING.store(false, Ordering::SeqCst);
                TAP_PTR.store(ptr::null_mut(), Ordering::SeqCst);
                if let Some(state) = TAP.lock().take() {
                    CFRunLoopRemoveSource(state.rl, state.source, kCFRunLoopCommonModes);
                    CGEventTapEnable(state.tap, false);
                    CFMachPortInvalidate(state.tap);
                    CFRelease(state.source as *const c_void);
                    CFRelease(state.tap as *const c_void);
                }
            })
            .map_err(|e| anyhow!("fn tap thread: {e}"))?;

        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {
                *THREAD.lock() = Some(handle);
                Ok(())
            }
            _ => {
                RUNNING.store(false, Ordering::SeqCst);
                Err(anyhow!(
                    "Could not listen for Fn. Grant Input Monitoring in System Settings."
                ))
            }
        }
    }

    pub fn stop() {
        RUNNING.store(false, Ordering::SeqCst);
        *ON_EDGE.lock() = None;
        if let Some(state) = TAP.lock().as_ref() {
            unsafe { CFRunLoopStop(state.rl) };
        }
        if let Some(handle) = THREAD.lock().take() {
            let _ = handle.join();
        }
        LAST_FN.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_without_start_is_ok() {
        stop();
        stop();
    }
}
