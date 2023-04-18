use alloc::boxed::Box;
use core::any::Any;
use core::ptr;

use unwind as uw;
use uw::{UnwindException, UnwindReasonCode};

// In case where multiple copies of std exist in a single process,
// we use address of this static variable to distinguish an exception raised by
// this copy and some other copy (which needs to be treated as foreign exception).
static CANARY: u8 = 0;

struct RustPanic(Box<dyn Any + Send>, DropGuard);

struct DropGuard;

#[repr(C)]
struct ExceptionWithPayload {
    exception: MaybeUninit<UnwindException>,
    payload: RustPanic,
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        // #[cfg(feature = "panic-handler")]
        // {
        //     drop_panic();
        // }
        core::intrinsics::abort();
    }
}

pub unsafe trait ExceptionTrait {
    const CLASS: [u8; 8];

    fn wrap(this: Self) -> *mut UnwindException;
    unsafe fn unwrap(ex: *mut UnwindException) -> Self;
}

unsafe impl ExceptionTrait for RustPanic {
    const CLASS: [u8; 8] = *b"MOZ\0RUST";

    fn wrap(this: Self) -> *mut UnwindException {
        Box::into_raw(Box::new(ExceptionWithPayload {
            exception: MaybeUninit::uninit(),
            payload: this,
        })) as *mut UnwindException
    }

    unsafe fn unwrap(ex: *mut UnwindException) -> Self {
        let ex = unsafe { Box::from_raw(ex as *mut ExceptionWithPayload) };
        ex.payload
    }
}

#[repr(C)]
struct Exception {
    _uwe: UnwindException,
    canary: *const u8,
    cause: Box<dyn Any + Send>,
}

pub unsafe fn panic(data: Box<dyn Any + Send>) -> u32 {
    extern "C" fn exception_cleanup(
        _unwind_code: UnwindReasonCode,
        exception: *mut UnwindException,
    ) {
        unsafe {
            let _: Box<Exception> = Box::from_raw(exception as *mut Exception);
            super::__rust_drop_panic();
        }
    }

    let exception = Box::new(Exception {
        _uwe: UnwindException {
            exception_class: rust_exception_class(),
            exception_cleanup,
            private: [0; uw::unwinder_private_data_size],
        },
        canary: &CANARY,
        cause: data,
    });

    let exception_param = Box::into_raw(exception) as *mut UnwindException;
    return uw::_Unwind_RaiseException(exception_param) as u32;
    //eprintln!("start panic begin_panic");
}


pub unsafe fn cleanup(ptr: *mut u8) -> Box<dyn Any + Send> {
    let exception = ptr as *mut UnwindException;
    if (*exception).exception_class != rust_exception_class() {
        uw::_Unwind_DeleteException(exception);
        super::__rust_foreign_exception();
    }

    let exception = exception.cast::<Exception>();
    // Just access the canary field, avoid accessing the entire `Exception` as
    // it can be a foreign Rust exception.
    let canary = ptr::addr_of!((*exception).canary).read();
    if !ptr::eq(canary, &CANARY) {
        // A foreign Rust exception, treat it slightly differently from other
        // foreign exceptions, because call into `_Unwind_DeleteException` will
        // call into `__rust_drop_panic` which produces a confusing
        // "Rust panic must be rethrown" message.
        super::__rust_foreign_exception();
    }

    let exception = Box::from_raw(exception as *mut Exception);
    exception.cause
}

fn rust_exception_class() -> u64 {
    // M O Z \0  R U S T -- vendor, language
    0x4d4f5a_00_52555354
}
