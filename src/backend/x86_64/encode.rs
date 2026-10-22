//! The x86-64 instruction encoder: the only file that contains opcode bytes.
//!
//! Every method appends exactly one instruction (or one fixed idiom) and
//! documents its encoding. Integer model reminder (see `mod.rs`): 32-bit values
//! live sign-extended in 64-bit registers, so most arithmetic is 64-bit and is
//! followed by `movsxd` where it can overflow.

use super::abi::Reg;

/// Condition for `setcc`; the value is the second opcode byte of `0F xx /0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond { Equal = 0x94, NotEqual = 0x95, Less = 0x9c, GreaterEqual = 0x9d, LessEqual = 0x9e, Greater = 0x9f }

#[derive(Debug, Default)]
pub struct Asm { bytes: Vec<u8> }

fn rex_w(reg_field: Reg, rm_field: Reg) -> u8 {
    0x48 | if reg_field >= 8 { 0x04 } else { 0 } | if rm_field >= 8 { 0x01 } else { 0 }
}

impl Asm {
    pub fn new() -> Asm { Asm::default() }
    pub fn len(&self) -> usize { self.bytes.len() }
    pub fn into_bytes(self) -> Vec<u8> { self.bytes }
    #[cfg(test)]
    pub fn bytes(&self) -> &[u8] { &self.bytes }
    pub fn bytes_mut(&mut self) -> &mut [u8] { &mut self.bytes }

    fn raw(&mut self, b: &[u8]) { self.bytes.extend_from_slice(b); }
    fn imm32(&mut self, v: u32) { self.raw(&v.to_le_bytes()); }

    // ---- frame and stack -------------------------------------------------
    /// `push rbp`
    pub fn push_rbp(&mut self) { self.raw(&[0x55]); }
    /// `pop rbp`
    pub fn pop_rbp(&mut self) { self.raw(&[0x5d]); }
    /// `mov rbp, rsp`
    pub fn mov_rbp_rsp(&mut self) { self.raw(&[0x48, 0x89, 0xe5]); }
    /// `mov rsp, rbp`
    pub fn mov_rsp_rbp(&mut self) { self.raw(&[0x48, 0x89, 0xec]); }
    /// `sub rsp, imm32`
    pub fn sub_rsp_imm32(&mut self, n: u32) { self.raw(&[0x48, 0x81, 0xec]); self.imm32(n); }
    /// `add rsp, imm32`
    pub fn add_rsp_imm32(&mut self, n: u32) { self.raw(&[0x48, 0x81, 0xc4]); self.imm32(n); }
    /// `sub rsp, imm8`
    pub fn sub_rsp_imm8(&mut self, n: u8) { self.raw(&[0x48, 0x83, 0xec, n]); }
    /// `push rax`
    pub fn push_rax(&mut self) { self.raw(&[0x50]); }
    /// `ret`
    pub fn ret(&mut self) { self.raw(&[0xc3]); }
    /// `syscall`
    pub fn syscall(&mut self) { self.raw(&[0x0f, 0x05]); }

    // ---- moves -----------------------------------------------------------
    /// `mov dst, src` (64-bit, register to register)
    pub fn mov_rr(&mut self, dst: Reg, src: Reg) {
        self.raw(&[rex_w(src, dst), 0x89, 0xc0 | ((src & 7) << 3) | (dst & 7)]);
    }
    /// `mov [rbp - offset], reg`. A negative `offset` addresses above RBP.
    pub fn store_frame(&mut self, reg: Reg, offset: i32) {
        self.raw(&[rex_w(reg, 0), 0x89, 0x85 | ((reg & 7) << 3)]);
        self.imm32((-offset) as u32);
    }
    /// `mov reg, [rbp - offset]`
    pub fn load_frame(&mut self, reg: Reg, offset: i32) {
        self.raw(&[rex_w(reg, 0), 0x8b, 0x85 | ((reg & 7) << 3)]);
        self.imm32((-offset) as u32);
    }
    /// `movsxd rax, dword [rbp - offset]`
    pub fn load_frame_dword_sx_rax(&mut self, offset: i32) {
        self.raw(&[0x48, 0x63, 0x85]);
        self.imm32((-offset) as u32);
    }
    /// `mov rax, imm64`
    pub fn mov_rax_imm64(&mut self, v: i64) { self.raw(&[0x48, 0xb8]); self.raw(&v.to_le_bytes()); }
    /// `mov eax, imm32`
    pub fn mov_eax_imm32(&mut self, v: u32) { self.raw(&[0xb8]); self.imm32(v); }
    /// `movsxd rax, eax`
    pub fn movsxd_rax_eax(&mut self) { self.raw(&[0x48, 0x63, 0xc0]); }
    /// `movsxd rax, edx`
    pub fn movsxd_rax_edx(&mut self) { self.raw(&[0x48, 0x63, 0xc2]); }

    // ---- arithmetic on rax, rcx -------------------------------------------
    pub fn add_rax_rcx(&mut self) { self.raw(&[0x48, 0x01, 0xc8]); }
    pub fn sub_rax_rcx(&mut self) { self.raw(&[0x48, 0x29, 0xc8]); }
    pub fn imul_rax_rcx(&mut self) { self.raw(&[0x48, 0x0f, 0xaf, 0xc1]); }
    pub fn and_rax_rcx(&mut self) { self.raw(&[0x48, 0x21, 0xc8]); }
    pub fn or_rax_rcx(&mut self) { self.raw(&[0x48, 0x09, 0xc8]); }
    pub fn xor_rax_rcx(&mut self) { self.raw(&[0x48, 0x31, 0xc8]); }
    pub fn cmp_rax_rcx(&mut self) { self.raw(&[0x48, 0x39, 0xc8]); }
    pub fn neg_rax(&mut self) { self.raw(&[0x48, 0xf7, 0xd8]); }
    pub fn not_rax(&mut self) { self.raw(&[0x48, 0xf7, 0xd0]); }
    pub fn test_rax_rax(&mut self) { self.raw(&[0x48, 0x85, 0xc0]); }
    /// `cdq` (sign-extend eax into edx:eax)
    pub fn cdq(&mut self) { self.raw(&[0x99]); }
    /// `idiv ecx` (32-bit signed divide; quotient in eax, remainder in edx)
    pub fn idiv_ecx(&mut self) { self.raw(&[0xf7, 0xf9]); }
    /// `setcc al; movzx rax, al`
    pub fn set_rax_if(&mut self, cond: Cond) {
        self.raw(&[0x0f, cond as u8, 0xc0]);
        self.raw(&[0x48, 0x0f, 0xb6, 0xc0]);
    }

    // ---- control flow ------------------------------------------------------
    /// `jmp rel32`; returns the offset of the displacement to patch.
    pub fn jmp_rel32(&mut self) -> usize { self.raw(&[0xe9]); self.placeholder() }
    /// `jz rel32`; returns the offset of the displacement to patch.
    pub fn jz_rel32(&mut self) -> usize { self.raw(&[0x0f, 0x84]); self.placeholder() }
    /// `call rel32`; returns the offset of the displacement to patch.
    pub fn call_rel32(&mut self) -> usize { self.raw(&[0xe8]); self.placeholder() }
    fn placeholder(&mut self) -> usize { let at = self.bytes.len(); self.raw(&[0; 4]); at }
}

/// Point the `rel32` displacement stored at `at` to `target` (both offsets into `code`).
pub fn patch_rel32(code: &mut [u8], at: usize, target: usize, what: &str) -> Result<(), String> {
    let delta = i32::try_from(target as i64 - (at + 4) as i64).map_err(|_| format!("{what} displacement out of range"))?;
    code[at..at + 4].copy_from_slice(&delta.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::abi::{R12, R15, R8, RAX, RBX, RCX, RDI, RDX};

    fn enc(f: impl FnOnce(&mut Asm)) -> Vec<u8> { let mut a = Asm::new(); f(&mut a); a.into_bytes() }

    #[test] fn frame_setup_and_return() {
        assert_eq!(enc(|a| { a.push_rbp(); a.mov_rbp_rsp(); }), [0x55, 0x48, 0x89, 0xe5]);
        assert_eq!(enc(|a| { a.mov_rsp_rbp(); a.pop_rbp(); a.ret(); }), [0x48, 0x89, 0xec, 0x5d, 0xc3]);
        assert_eq!(enc(|a| a.sub_rsp_imm32(0x80)), [0x48, 0x81, 0xec, 0x80, 0, 0, 0]);
        assert_eq!(enc(|a| a.add_rsp_imm32(0x10)), [0x48, 0x81, 0xc4, 0x10, 0, 0, 0]);
        assert_eq!(enc(|a| a.sub_rsp_imm8(8)), [0x48, 0x83, 0xec, 0x08]);
        assert_eq!(enc(|a| a.push_rax()), [0x50]);
    }
    #[test] fn register_to_register_moves_handle_extended_registers() {
        assert_eq!(enc(|a| a.mov_rr(RDI, RAX)), [0x48, 0x89, 0xc7]);   // mov rdi, rax
        assert_eq!(enc(|a| a.mov_rr(R8, RBX)), [0x49, 0x89, 0xd8]);    // mov r8, rbx
        assert_eq!(enc(|a| a.mov_rr(RAX, R12)), [0x4c, 0x89, 0xe0]);   // mov rax, r12
        assert_eq!(enc(|a| a.mov_rr(R15, R12)), [0x4d, 0x89, 0xe7]);   // mov r15, r12
        assert_eq!(enc(|a| a.mov_rr(RCX, RAX)), [0x48, 0x89, 0xc1]);   // mov rcx, rax
    }
    #[test] fn frame_slot_loads_and_stores() {
        assert_eq!(enc(|a| a.store_frame(RAX, 8)), [0x48, 0x89, 0x85, 0xf8, 0xff, 0xff, 0xff]);   // mov [rbp-8], rax
        assert_eq!(enc(|a| a.load_frame(RCX, 16)), [0x48, 0x8b, 0x8d, 0xf0, 0xff, 0xff, 0xff]);   // mov rcx, [rbp-16]
        assert_eq!(enc(|a| a.store_frame(R12, 24)), [0x4c, 0x89, 0xa5, 0xe8, 0xff, 0xff, 0xff]);  // mov [rbp-24], r12
        assert_eq!(enc(|a| a.load_frame(RDX, -16)), [0x48, 0x8b, 0x95, 0x10, 0, 0, 0]);           // mov rdx, [rbp+16]
        assert_eq!(enc(|a| a.load_frame_dword_sx_rax(8)), [0x48, 0x63, 0x85, 0xf8, 0xff, 0xff, 0xff]);
    }
    #[test] fn immediates_and_sign_extensions() {
        assert_eq!(enc(|a| a.mov_rax_imm64(-1)), [0x48, 0xb8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        assert_eq!(enc(|a| a.mov_eax_imm32(60)), [0xb8, 60, 0, 0, 0]);
        assert_eq!(enc(|a| a.movsxd_rax_eax()), [0x48, 0x63, 0xc0]);
        assert_eq!(enc(|a| a.movsxd_rax_edx()), [0x48, 0x63, 0xc2]);
    }
    #[test] fn arithmetic_and_compare() {
        assert_eq!(enc(|a| { a.add_rax_rcx(); a.sub_rax_rcx(); a.imul_rax_rcx(); }),
            [0x48, 0x01, 0xc8, 0x48, 0x29, 0xc8, 0x48, 0x0f, 0xaf, 0xc1]);
        assert_eq!(enc(|a| { a.and_rax_rcx(); a.or_rax_rcx(); a.xor_rax_rcx(); }),
            [0x48, 0x21, 0xc8, 0x48, 0x09, 0xc8, 0x48, 0x31, 0xc8]);
        assert_eq!(enc(|a| { a.neg_rax(); a.not_rax(); a.test_rax_rax(); a.cmp_rax_rcx(); }),
            [0x48, 0xf7, 0xd8, 0x48, 0xf7, 0xd0, 0x48, 0x85, 0xc0, 0x48, 0x39, 0xc8]);
        assert_eq!(enc(|a| { a.cdq(); a.idiv_ecx(); }), [0x99, 0xf7, 0xf9]);
    }
    #[test] fn setcc_variants() {
        for (cond, byte) in [(Cond::Equal, 0x94), (Cond::NotEqual, 0x95), (Cond::Less, 0x9c),
                             (Cond::LessEqual, 0x9e), (Cond::Greater, 0x9f), (Cond::GreaterEqual, 0x9d)] {
            assert_eq!(enc(|a| a.set_rax_if(cond)), [0x0f, byte, 0xc0, 0x48, 0x0f, 0xb6, 0xc0]);
        }
    }
    #[test] fn jumps_and_calls_return_the_displacement_offset() {
        let mut a = Asm::new();
        a.mov_eax_imm32(1);
        assert_eq!(a.jmp_rel32(), 6);   // e9 at 5, displacement 6..10
        assert_eq!(a.jz_rel32(), 12);   // 0f 84 at 10..12, displacement 12..16
        assert_eq!(a.call_rel32(), 17); // e8 at 16, displacement 17..21
        assert_eq!(a.bytes()[5], 0xe9);
        assert_eq!(&a.bytes()[10..12], [0x0f, 0x84]);
        assert_eq!(a.bytes()[16], 0xe8);
        assert_eq!(a.len(), 21);
    }
    #[test] fn patching_a_displacement() {
        let mut a = Asm::new();
        let at = a.jmp_rel32();            // jmp to offset 0: displacement = 0 - (1 + 4)
        patch_rel32(a.bytes_mut(), at, 0, "branch").unwrap();
        assert_eq!(a.bytes(), [0xe9, 0xfb, 0xff, 0xff, 0xff]);
        let mut far = vec![0u8; 8];
        assert!(patch_rel32(&mut far, 0, usize::MAX / 2, "call").unwrap_err().contains("call displacement out of range"));
    }
    #[test] fn entry_stub_idiom() {
        assert_eq!(enc(|a| { a.mov_rr(RDI, RAX); a.mov_eax_imm32(60); a.syscall(); }),
            [0x48, 0x89, 0xc7, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
    }
}
