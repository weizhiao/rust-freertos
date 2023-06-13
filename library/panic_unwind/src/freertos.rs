use alloc::boxed::Box;
use core::any::Any;
use core::ptr;

use unwind as uw;
use uw::{UnwindException, UnwindReasonCode};

// In case where multiple copies of std exist in a single process,
// we use address of this static variable to distinguish an exception raised by
// this copy and some other copy (which needs to be treated as foreign exception).
static CANARY: u8 = 0;

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

    let mut exception = Box::new(Exception {
        _uwe: UnwindException {
            exception_class: rust_exception_class(),
            exception_cleanup:Some(exception_cleanup),
            private_1: None,
            private_2:0,
            private_unused:[]
        },
        canary: &CANARY,
        cause: data,
    });

    return uw::_Unwind_RaiseException(&mut exception._uwe).0 as u32;
}


pub unsafe fn cleanup(ptr: *mut u8) -> Box<dyn Any + Send> {
    let exception = ptr as *mut UnwindException;
    if (*exception).exception_class != rust_exception_class() {
        uw::_Unwind_DeleteException(exception);
        super::__rust_foreign_exception();
    }

    let exception = exception.cast::<Exception>();

    let canary = ptr::addr_of!((*exception).canary).read();
    if !ptr::eq(canary, &CANARY) {
        super::__rust_foreign_exception();
    }

    let exception = Box::from_raw(exception as *mut Exception);
    exception.cause
}

fn rust_exception_class() -> u64 {
    // M O Z \0  R U S T -- vendor, language
    0x4d4f5a_00_52555354
}
