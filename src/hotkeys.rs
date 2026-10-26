//! System-wide presenter clicker keys, so a clicker works even when SimplePDF isn't focused.
//!
//! Windows only: PageUp/PageDown and Left/Right are registered as global hotkeys while enabled
//! (the app enables them while the output is shown). Registered keys are taken away from all
//! other programs, which is why B/./Esc stay focus-only.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::command::Command;

pub struct GlobalKeys {
    /// Number of keys currently registered (0 when disabled or registration failed).
    registered: Arc<AtomicUsize>,
    enabled: bool,
    #[cfg(windows)]
    thread_id: u32,
}

impl GlobalKeys {
    pub fn registered(&self) -> usize {
        self.registered.load(Ordering::Relaxed)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(not(windows))]
impl GlobalKeys {
    pub fn spawn(_send: impl Fn(Command) + Send + 'static) -> Option<Self> {
        None
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }
}

#[cfg(windows)]
mod win {
    use std::ptr::null_mut;
    use std::sync::mpsc;

    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY, WM_USER,
    };

    use super::*;

    const WM_ENABLE_KEYS: u32 = WM_APP + 1;
    const WM_DISABLE_KEYS: u32 = WM_APP + 2;

    const KEYS: [(u16, fn() -> Command); 4] = [
        (VK_NEXT, || Command::NextPage),
        (VK_RIGHT, || Command::NextPage),
        (VK_PRIOR, || Command::PrevPage),
        (VK_LEFT, || Command::PrevPage),
    ];

    impl GlobalKeys {
        pub fn spawn(send: impl Fn(Command) + Send + 'static) -> Option<Self> {
            let registered = Arc::new(AtomicUsize::new(0));
            let count = registered.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new()
                .name("hotkeys".into())
                .spawn(move || unsafe {
                    // Creates the thread's message queue before anyone posts to it.
                    let mut msg: MSG = std::mem::zeroed();
                    PeekMessageW(&mut msg, null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
                    let _ = tx.send(GetCurrentThreadId());

                    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                        match msg.message {
                            WM_HOTKEY => {
                                if let Some((_, cmd)) = KEYS.get(msg.wParam.wrapping_sub(1)) {
                                    send(cmd());
                                }
                            }
                            WM_ENABLE_KEYS => {
                                let mut n = 0;
                                for (i, (vk, _)) in KEYS.iter().enumerate() {
                                    if RegisterHotKey(
                                        null_mut(),
                                        i as i32 + 1,
                                        MOD_NOREPEAT,
                                        *vk as u32,
                                    ) != 0
                                    {
                                        n += 1;
                                    }
                                }
                                count.store(n, Ordering::Relaxed);
                            }
                            WM_DISABLE_KEYS => {
                                for i in 0..KEYS.len() {
                                    UnregisterHotKey(null_mut(), i as i32 + 1);
                                }
                                count.store(0, Ordering::Relaxed);
                            }
                            _ => {}
                        }
                    }
                })
                .ok()?;
            let thread_id = rx.recv().ok()?;
            Some(Self {
                registered,
                enabled: false,
                thread_id,
            })
        }

        pub fn set_enabled(&mut self, on: bool) {
            if self.enabled == on {
                return;
            }
            self.enabled = on;
            let msg = if on { WM_ENABLE_KEYS } else { WM_DISABLE_KEYS };
            unsafe {
                PostThreadMessageW(self.thread_id, msg, 0, 0);
            }
        }
    }
}
