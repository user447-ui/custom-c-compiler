//! Minimal ELF64 little-endian executable writer: one `PT_LOAD` segment that
//! maps the whole file (headers and code) read+execute at [`BASE`].

const BASE: u64 = 0x400000;
const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;
/// Machine code starts right after the two headers.
const CODE_OFFSET: usize = EHDR_SIZE + PHDR_SIZE;

const ET_EXEC: u16 = 2;
const PT_LOAD: u32 = 1;
const PF_R_X: u32 = 5; // PF_R | PF_X
const PAGE: u64 = 0x1000;

struct Writer { bytes: Vec<u8> }
impl Writer {
    fn u16_at(&mut self, at: usize, v: u16) { self.bytes[at..at + 2].copy_from_slice(&v.to_le_bytes()); }
    fn u32_at(&mut self, at: usize, v: u32) { self.bytes[at..at + 4].copy_from_slice(&v.to_le_bytes()); }
    fn u64_at(&mut self, at: usize, v: u64) { self.bytes[at..at + 8].copy_from_slice(&v.to_le_bytes()); }
}

/// Build an executable for `e_machine` whose entry point is `entry_offset`
/// bytes into `machine_code`.
pub fn executable(e_machine: u16, machine_code: &[u8], entry_offset: usize) -> Vec<u8> {
    let size = CODE_OFFSET + machine_code.len();
    let mut w = Writer { bytes: vec![0u8; size] };
    // ELF header
    w.bytes[..4].copy_from_slice(b"\x7fELF");
    w.bytes[4] = 2; // ELFCLASS64
    w.bytes[5] = 1; // ELFDATA2LSB
    w.bytes[6] = 1; // EV_CURRENT
    w.u16_at(16, ET_EXEC);
    w.u16_at(18, e_machine);
    w.u32_at(20, 1); // e_version
    w.u64_at(24, BASE + (CODE_OFFSET + entry_offset) as u64); // e_entry
    w.u64_at(32, EHDR_SIZE as u64); // e_phoff
    w.u16_at(52, EHDR_SIZE as u16); // e_ehsize
    w.u16_at(54, PHDR_SIZE as u16); // e_phentsize
    w.u16_at(56, 1); // e_phnum
    // Program header
    w.u32_at(64, PT_LOAD);
    w.u32_at(68, PF_R_X);
    w.u64_at(72, 0); // p_offset: map from the start of the file
    w.u64_at(80, BASE); // p_vaddr
    w.u64_at(88, BASE); // p_paddr
    w.u64_at(96, size as u64); // p_filesz
    w.u64_at(104, size as u64); // p_memsz
    w.u64_at(112, PAGE); // p_align
    w.bytes[CODE_OFFSET..].copy_from_slice(machine_code);
    w.bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    fn u16_at(b: &[u8], at: usize) -> u16 { u16::from_le_bytes([b[at], b[at + 1]]) }
    fn u32_at(b: &[u8], at: usize) -> u32 { u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) }
    fn u64_at(b: &[u8], at: usize) -> u64 { u64::from_le_bytes(b[at..at + 8].try_into().unwrap()) }

    #[test] fn writes_a_valid_elf64_header() {
        let b = executable(62, &[0xc3], 0);
        assert_eq!(&b[..4], b"\x7fELF");
        assert_eq!((b[4], b[5], b[6]), (2, 1, 1));
        assert_eq!((u16_at(&b, 16), u16_at(&b, 18), u32_at(&b, 20)), (ET_EXEC, 62, 1));
        assert_eq!((u16_at(&b, 52), u16_at(&b, 54), u16_at(&b, 56)), (64, 56, 1));
    }
    #[test] fn entry_point_follows_the_headers_plus_offset() {
        assert_eq!(u64_at(&executable(62, &[0; 10], 0), 24), BASE + 120);
        assert_eq!(u64_at(&executable(62, &[0; 10], 7), 24), BASE + 127);
    }
    #[test] fn one_load_segment_covers_the_whole_file() {
        let b = executable(62, &[1, 2, 3], 0);
        assert_eq!(u32_at(&b, 64), PT_LOAD);
        assert_eq!(u32_at(&b, 68), PF_R_X);
        assert_eq!((u64_at(&b, 72), u64_at(&b, 80), u64_at(&b, 88)), (0, BASE, BASE));
        assert_eq!((u64_at(&b, 96), u64_at(&b, 104)), (b.len() as u64, b.len() as u64));
        assert_eq!(u64_at(&b, 112), PAGE);
    }
    #[test] fn code_is_placed_after_the_headers_and_machine_is_a_parameter() {
        let b = executable(183, &[0xde, 0xad], 0);
        assert_eq!(&b[120..], [0xde, 0xad]);
        assert_eq!(u16_at(&b, 18), 183);
    }
}
