#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "riscv64")]
pub use riscv64::*;

#[cfg(not(any(
    target_arch = "riscv64"
)))]
compile_error!("Current architecture is not supported");
