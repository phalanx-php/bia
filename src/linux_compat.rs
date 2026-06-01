use libc::{c_int, c_void, size_t};

extern "C" {
    #[link_name = "__sigsetjmp"]
    fn glibc_sigsetjmp(env: *mut c_void, savemask: c_int) -> c_int;

    #[link_name = "__res_init"]
    fn glibc_res_init() -> c_int;
}

#[no_mangle]
pub unsafe extern "C" fn sigsetjmp(env: *mut c_void, savemask: c_int) -> c_int {
    unsafe { glibc_sigsetjmp(env, savemask) }
}

#[no_mangle]
pub unsafe extern "C" fn res_init() -> c_int {
    unsafe { glibc_res_init() }
}

#[export_name = "_ZNSt3__113__hash_memoryEPKvm"]
pub unsafe extern "C" fn dory_libcpp_hash_memory(key: *const c_void, len: size_t) -> size_t {
    let bytes = unsafe { std::slice::from_raw_parts(key.cast::<u8>(), len) };
    let mut hash = 14_695_981_039_346_656_037usize;

    for byte in bytes {
        hash ^= usize::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211usize);
    }

    hash
}
