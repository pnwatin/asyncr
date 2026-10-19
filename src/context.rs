use std::cell::RefCell;

use crate::Handle;

thread_local! {
    static CONTEXT: RefCell<Option<Handle>> =  const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetCurrentError {
    AlreadyEntered,
}

pub(crate) fn try_set_current(handle: &Handle) -> Result<(), SetCurrentError> {
    CONTEXT.with_borrow_mut(|ctx| match ctx {
        Some(_) => Err(SetCurrentError::AlreadyEntered),
        None => {
            ctx.replace(handle.clone());
            Ok(())
        }
    })
}

pub(crate) fn clear_current() {
    CONTEXT.with_borrow_mut(|ctx| *ctx = None);
}

pub(crate) fn try_current() -> Option<Handle> {
    CONTEXT.with_borrow(|ctx| ctx.clone())
}

#[track_caller]
pub(crate) fn current() -> Handle {
    try_current().expect("no asyncr runtime is currently entered on this thread")
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
        thread,
    };

    use crate::Runtime;

    use super::*;

    fn assert_same_runtime(actual: &Handle, expected: &Handle) {
        assert!(Rc::ptr_eq(&actual.spawn_state, &expected.spawn_state));
    }

    #[test]
    fn no_runtime_is_current_initially() {
        assert!(try_current().is_none());
    }

    #[test]
    #[should_panic(expected = "no asyncr runtime is currently entered on this thread")]
    fn current_panics_outside_an_entered_runtime() {
        current();
    }

    #[test]
    fn guard_makes_handle_current_until_dropped() {
        let runtime = Runtime::new();
        let handle = runtime.handle();

        let guard = handle.enter();

        let current = current();
        assert_same_runtime(&current, &handle);

        drop(guard);

        assert!(try_current().is_none());
    }

    #[test]
    fn nested_entry_is_rejected_without_replacing_outer_context() {
        let outer_runtime = Runtime::new();
        let outer_handle = outer_runtime.handle();
        let inner_runtime = Runtime::new();
        let inner_handle = inner_runtime.handle();
        let outer_guard = outer_handle.enter();

        assert_eq!(
            try_set_current(&inner_handle),
            Err(SetCurrentError::AlreadyEntered)
        );

        let nested_entry = catch_unwind(AssertUnwindSafe(|| inner_handle.enter()));
        assert!(nested_entry.is_err());
        assert_same_runtime(&current(), &outer_handle);

        drop(outer_guard);

        assert!(try_current().is_none());
    }

    #[test]
    fn guard_clears_context_during_panic_unwinding() {
        let runtime = Runtime::new();
        let handle = runtime.handle();

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = handle.enter();
            panic!("root future panicked");
        }));

        assert!(result.is_err());
        assert!(try_current().is_none());
    }

    #[test]
    fn context_is_local_to_each_thread() {
        let runtime = Runtime::new();
        let handle = runtime.handle();
        let _guard = handle.enter();

        thread::spawn(|| {
            assert!(try_current().is_none());

            let runtime = Runtime::new();
            let handle = runtime.handle();
            let _guard = handle.enter();

            assert_same_runtime(&current(), &handle);
        })
        .join()
        .unwrap();

        assert_same_runtime(&current(), &handle);
    }
}
