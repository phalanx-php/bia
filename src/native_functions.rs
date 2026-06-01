use std::os::raw::{c_char, c_void};

use ripht_php_sapi::native;
use ripht_php_sapi::native::{Call, Function, ReturnValue};

use crate::analysis;

unsafe extern "C" fn zif_dory_code_query_json(
    execute_data: *mut c_void,
    return_value: *mut ReturnValue,
) {
    let json = std::panic::catch_unwind(|| {
        // SAFETY: PHP supplied `execute_data` for this active native call.
        let call = unsafe { Call::from_execute_data(execute_data) };
        let argc = call.num_args();

        if argc == 0 {
            return analysis::error_json("dory_code_query_json expects a JSON request");
        }

        let Some(request) = call.arg_string(1) else {
            return analysis::error_json("request must be a string");
        };

        analysis::CodeQueryEngine::shared().dispatch_json(&String::from_utf8_lossy(request))
    })
    .unwrap_or_else(|_| analysis::error_json("Dory code parser panicked"));

    // SAFETY: PHP supplied `return_value` for this active native call.
    unsafe { native::set_string(return_value, json.as_bytes()) };
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ZendType {
    ptr: *const c_void,
    type_mask: u32,
    _padding: u32,
}

#[repr(C)]
struct ArgInfo {
    name: *const c_char,
    type_info: ZendType,
    default_value: *const c_char,
}

// SAFETY: arginfo rows are immutable process-lifetime static metadata.
unsafe impl Sync for ArgInfo {}

const IS_STRING: u32 = 6;
const ZEND_NULLABLE: u32 = 0x2;

const fn type_info(code: u32, allow_null: bool) -> ZendType {
    ZendType {
        ptr: std::ptr::null(),
        type_mask: (1 << code) | if allow_null { ZEND_NULLABLE } else { 0 },
        _padding: 0,
    }
}

static ARGINFO_CODE_QUERY_JSON: [ArgInfo; 2] = [
    ArgInfo {
        name: std::ptr::dangling::<c_char>(),
        type_info: type_info(IS_STRING, false),
        default_value: std::ptr::null(),
    },
    ArgInfo {
        name: c"request".as_ptr(),
        type_info: type_info(IS_STRING, false),
        default_value: std::ptr::null(),
    },
];

macro_rules! func_entry {
    ($name:expr, $handler:expr, $arginfo:expr) => {
        // SAFETY: function names and arginfo point to immutable static data, and
        // handlers use PHP's native function calling convention.
        unsafe {
            Function::new_unchecked(
                $name.as_ptr(),
                $handler,
                $arginfo.as_ptr() as *const c_void,
                ($arginfo.len() - 1) as u32,
            )
        }
    };
}

pub fn entries() -> &'static [Function] {
    static ENTRIES: [Function; 1] = [func_entry!(
        c"dory_code_query_json",
        zif_dory_code_query_json,
        ARGINFO_CODE_QUERY_JSON
    )];

    &ENTRIES
}
