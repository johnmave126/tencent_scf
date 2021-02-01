//! This module contains various helpers used in the library

use std::{
    panic,
    sync::{Arc, Mutex},
};

/// A RAII struct to redirect panic message.
///
/// The guard is a RAII struct that stores the old panic hook on creation and restore the hook on
/// drop. Any panic message during its lifetime will be redirect to an internal buffer which can be
/// read out.
///
/// # Example
/// ```rust,ignore
/// use tencent_scf::helper::PanicGuard;
/// use std::panic;
/// {
///     let guard = PanicGuard::new();
///     let _ = panic::catch_unwind(|| {
///         // panic message is not printed to stderr. Instead it is stored by the guard.
///         panic!("panic message");
///     });
///     assert!(guard.get_panic().contains("panic message"));
/// }
/// let _ = panic::catch_unwind(|| {
///     // panic message should be printed to the stderr.
///     panic!("panic message");
/// });
/// ```
pub(crate) struct PanicGuard {
    message: Arc<Mutex<String>>,
    old_hook: Box<dyn Fn(&panic::PanicInfo<'_>) + Sync + Send + 'static>,
}

impl PanicGuard {
    /// Create the guard. This also change the panic handler.
    pub(crate) fn new() -> Self {
        let message = Arc::new(Mutex::new(String::new()));

        let old_hook = panic::take_hook();
        panic::set_hook({
            let message = message.clone();
            Box::new(move |info: &panic::PanicInfo<'_>| {
                let panic_payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
                    *s
                } else if let Some(s) = info.payload().downcast_ref::<String>() {
                    s
                } else {
                    "<Non-String Panic Payload>"
                };
                let panic_message = if let Some(location) = info.location() {
                    format!(
                        "function panicked in file '{}' at line {}: {}\n",
                        location.file(),
                        location.line(),
                        panic_payload
                    )
                } else {
                    format!(
                        "function panicked, but location unavailable: {}\n",
                        panic_payload
                    )
                };

                if let Ok(mut message) = message.lock() {
                    message.push_str(&panic_message);
                }
            })
        });
        Self { message, old_hook }
    }

    /// Get the panic message of this guard. This function *never* panics.
    pub(crate) fn get_panic(&self) -> String {
        self.message
            .lock()
            .map(|message| message.clone())
            .unwrap_or_else(|_| {
                "function panicked, but unable to retrieve further information".to_string()
            })
    }
}

impl Drop for PanicGuard {
    /// Restore the panic handler to the saved one
    fn drop(&mut self) {
        let mut current_hook = panic::take_hook();
        std::mem::swap(&mut self.old_hook, &mut current_hook);
        panic::set_hook(current_hook);
    }
}

#[cfg(test)]
mod test {
    use std::panic::catch_unwind;

    #[test]
    fn panic_message_redirect() {
        let guard = super::PanicGuard::new();
        let _ = catch_unwind(|| {
            panic!("panic message");
        });
        assert!(guard.get_panic().contains("panic message"));
    }

    #[test]
    fn panic_hook_restore() {
        let outer_guard = super::PanicGuard::new();
        {
            let inner_guard = super::PanicGuard::new();
            let _ = catch_unwind(|| {
                panic!("inner panic message");
            });
            assert!(inner_guard.get_panic().contains("inner panic message"));
        }
        let _ = catch_unwind(|| {
            panic!("outer panic message");
        });
        assert!(outer_guard.get_panic().contains("outer panic message"));
    }
}
