//! cirka-runtime: the shared runtime for aCHC and CirkaLang (`libcirka_rt.a`).
#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
