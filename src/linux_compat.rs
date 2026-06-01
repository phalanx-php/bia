use libc::{c_int, c_void, size_t};

unsafe extern "C" {
    #[link_name = "__sigsetjmp"]
    fn glibc_sigsetjmp(env: *mut c_void, savemask: c_int) -> c_int;

    #[link_name = "__res_init"]
    fn glibc_res_init() -> c_int;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigsetjmp(env: *mut c_void, savemask: c_int) -> c_int {
    unsafe { glibc_sigsetjmp(env, savemask) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn res_init() -> c_int {
    unsafe { glibc_res_init() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn issetugid() -> c_int {
    // Fake the BSD issetugid check (returning 0 means "not setuid/setgid")
    0
}
