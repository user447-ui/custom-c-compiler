//! Executable file formats. The format follows from the operating system, the
//! machine number inside it from the architecture.

pub mod elf;

use crate::target::{Os, Target};

/// Wrap `code` (which starts at the program entry point) into an executable
/// file for `target`.
pub fn executable(target: &Target, code: &[u8]) -> Result<Vec<u8>, String> {
    match target.os {
        Os::Linux => Ok(elf::executable(target.arch.elf_machine(), code, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn linux_targets_get_elf_with_the_architectures_machine_number() {
        let file = executable(&Target::DEFAULT, &[0xc3]).unwrap();
        assert_eq!(&file[..4], b"\x7fELF");
        assert_eq!(u16::from_le_bytes([file[18], file[19]]), 62);
    }
}
