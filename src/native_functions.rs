use std::os::raw::{c_char, c_void};

use ripht_php_sapi::native;
use ripht_php_sapi::native::{Call, Function, ReturnValue};

use crate::analysis;

unsafe extern "C" fn zif_dory_mago_parse_json(
    execute_data: *mut c_void,
    return_value: *mut ReturnValue,
) {
    let json = std::panic::catch_unwind(|| {
        // SAFETY: PHP supplied `execute_data` for this active native call.
        let call = unsafe { Call::from_execute_data(execute_data) };
        let argc = call.num_args();

        if argc == 0 {
            return analysis::error_json(
                "dory_mago_parse_json expects source code as the first argument",
            );
        }

        let Some(source) = call.arg_string(1) else {
            return analysis::error_json("source must be a string");
        };

        let name = if argc >= 2 {
            match call.arg_string(2) {
                Some(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
                None => return analysis::error_json("name must be a string when provided"),
            }
        } else {
            None
        };

        analysis::parse_source_json(&String::from_utf8_lossy(source), name.as_deref())
    })
    .unwrap_or_else(|_| analysis::error_json("Dory Mago parser panicked"));

    // SAFETY: PHP supplied `return_value` for this active native call.
    unsafe { native::set_string(return_value, json.as_bytes()) };
}

unsafe extern "C" fn zif_dory_mago_parse_file_json(
    execute_data: *mut c_void,
    return_value: *mut ReturnValue,
) {
    let json = std::panic::catch_unwind(|| {
        // SAFETY: PHP supplied `execute_data` for this active native call.
        let call = unsafe { Call::from_execute_data(execute_data) };
        let argc = call.num_args();

        if argc == 0 {
            return analysis::error_json(
                "dory_mago_parse_file_json expects a path as the first argument",
            );
        }

        let Some(path) = call.arg_string(1) else {
            return analysis::error_json("path must be a string");
        };

        analysis::parse_file_json(&String::from_utf8_lossy(path))
    })
    .unwrap_or_else(|_| analysis::error_json("Dory Mago parser panicked"));

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

static ARGINFO_PARSE_JSON: [ArgInfo; 3] = [
    ArgInfo {
        name: 1usize as *const c_char,
        type_info: type_info(IS_STRING, false),
        default_value: std::ptr::null(),
    },
    ArgInfo {
        name: b"source\0".as_ptr() as *const c_char,
        type_info: type_info(IS_STRING, false),
        default_value: std::ptr::null(),
    },
    ArgInfo {
        name: b"name\0".as_ptr() as *const c_char,
        type_info: type_info(IS_STRING, true),
        default_value: b"null\0".as_ptr() as *const c_char,
    },
];

static ARGINFO_PARSE_FILE_JSON: [ArgInfo; 2] = [
    ArgInfo {
        name: 1usize as *const c_char,
        type_info: type_info(IS_STRING, false),
        default_value: std::ptr::null(),
    },
    ArgInfo {
        name: b"path\0".as_ptr() as *const c_char,
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
                $name.as_ptr() as *const c_char,
                $handler,
                $arginfo.as_ptr() as *const c_void,
                ($arginfo.len() - 1) as u32,
            )
        }
    };
}

pub fn entries() -> &'static [Function] {
    static ENTRIES: [Function; 2] = [
        func_entry!(
            b"dory_mago_parse_json\0",
            zif_dory_mago_parse_json,
            ARGINFO_PARSE_JSON
        ),
        func_entry!(
            b"dory_mago_parse_file_json\0",
            zif_dory_mago_parse_file_json,
            ARGINFO_PARSE_FILE_JSON
        ),
    ];

    &ENTRIES
}
