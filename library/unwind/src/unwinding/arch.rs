#[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))]
mod riscv {
    use gimli::{Register, RiscV};

    pub struct Arch;

    #[allow(unused)]
    impl Arch {
        pub const SP: Register = RiscV::SP;
        pub const RA: Register = RiscV::RA;

        pub const UNWIND_DATA_REG: (Register, Register) = (RiscV::A0, RiscV::A1);
        pub const UNWIND_PRIVATE_DATA_SIZE: usize = 2;
    }
}
#[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))]
pub use riscv::*;

#[cfg(not(any(
    target_arch = "riscv64",
    target_arch = "riscv32"
)))]
compile_error!("Current architecture is not supported");
