//! x86-64 registers and the System V AMD64 calling convention, as the rest of
//! the backend uses them. Nothing else in the backend writes a register number.

use crate::regalloc::PhysReg;
use crate::target::Os;

pub type Reg = PhysReg;

pub const RAX: Reg = 0;
pub const RCX: Reg = 1;
pub const RDX: Reg = 2;
pub const RBX: Reg = 3;
#[allow(dead_code)] // part of the register set the compile-time check below refers to
pub const RSP: Reg = 4;
#[allow(dead_code)]
pub const RBP: Reg = 5;
pub const RSI: Reg = 6;
pub const RDI: Reg = 7;
pub const R8: Reg = 8;
pub const R9: Reg = 9;
pub const R12: Reg = 12;
pub const R13: Reg = 13;
pub const R14: Reg = 14;
pub const R15: Reg = 15;

/// Integer argument registers, in order. Further arguments go on the stack.
pub const ARG_REGS: [Reg; 6] = [RDI, RSI, RDX, RCX, R8, R9];

/// Registers the callee must preserve. (rustc's dead-code lint does not see the use in
/// the compile-time check below.)
#[allow(dead_code)]
pub const CALLEE_SAVED: [Reg; 6] = [RBX, RBP, R12, R13, R14, R15];

/// Temporaries used by instruction selection inside a single IR instruction;
/// never allocated to long-lived values.
#[allow(dead_code)]
pub const SCRATCH: [Reg; 3] = [RAX, RCX, RDX];

/// Registers handed to the register allocator. All of them are callee-saved,
/// so a value kept in one survives calls; each function saves and restores
/// only the ones it uses.
pub const ALLOCATABLE_REGISTERS: [Reg; 5] = [RBX, R12, R13, R14, R15];

#[allow(dead_code)]
const fn contains(list: &[Reg], reg: Reg) -> bool {
    let mut i = 0;
    while i < list.len() {
        if list[i] == reg { return true; }
        i += 1;
    }
    false
}

/// Instruction selection clobbers `SCRATCH` freely, argument registers are loaded just
/// before a call, and RSP/RBP address the frame, so none of those may hold a value
/// across instructions; and a value must survive calls, so every allocatable register
/// must be callee-saved. Checked when the compiler is built.
#[allow(dead_code)]
const fn allocatable_registers_are_safe() -> bool {
    let mut i = 0;
    while i < ALLOCATABLE_REGISTERS.len() {
        let r = ALLOCATABLE_REGISTERS[i];
        if !contains(&CALLEE_SAVED, r) || contains(&SCRATCH, r) || contains(&ARG_REGS, r) || r == RSP || r == RBP {
            return false;
        }
        i += 1;
    }
    true
}
const _: () = assert!(allocatable_registers_are_safe(), "ALLOCATABLE_REGISTERS must be callee-saved and not reserved");

/// Assembly name of a register number.
pub fn register_name(reg: Reg) -> &'static str {
    const NAMES: [&str; 16] = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
        "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
    NAMES.get(reg as usize).copied().unwrap_or("?")
}

/// Number of the `exit` system call on this operating system (Linux x86-64: 60).
pub fn exit_syscall(os: Os) -> u32 {
    match os { Os::Linux => 60 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn compile_time_check_rejects_unsafe_sets() {
        // The const assertion above guards the real set; this exercises `contains`.
        assert!(contains(&CALLEE_SAVED, RBX) && !contains(&CALLEE_SAVED, RAX));
        assert!(contains(&SCRATCH, RCX) && !contains(&SCRATCH, RBX));
    }
    #[test] fn argument_registers_are_distinct_and_named() {
        let names: Vec<&str> = ARG_REGS.iter().map(|r| register_name(*r)).collect();
        assert_eq!(names, ["rdi", "rsi", "rdx", "rcx", "r8", "r9"]);
    }
    #[test] fn names_cover_all_sixteen_registers() {
        assert_eq!((0..16).map(register_name).filter(|n| *n == "?").count(), 0);
        assert_eq!(register_name(16), "?");
        assert_eq!((register_name(RBX), register_name(R15)), ("rbx", "r15"));
    }
    #[test] fn linux_exit_is_syscall_60() { assert_eq!(exit_syscall(Os::Linux), 60); }
}
