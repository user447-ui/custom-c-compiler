//! What the compiler generates code *for*: a CPU architecture and an operating
//! system, written like the first two parts of a target triple (`x86_64-linux`).
//!
//! The front end and the IR do not depend on the target. Everything that does
//! lives behind [`crate::backend::Backend`] (architecture) and
//! [`crate::object`] (executable file format, chosen by operating system).
//! Adding a variant here makes the compiler point at every `match` that must
//! learn about it.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch { X86_64 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os { Linux }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target { pub arch: Arch, pub os: Os }

impl Arch {
    pub const ALL: [Arch; 1] = [Arch::X86_64];
    pub fn name(self) -> &'static str { match self { Arch::X86_64 => "x86_64" } }
    /// Human-readable spelling used in messages.
    pub fn pretty(self) -> &'static str { match self { Arch::X86_64 => "x86-64" } }
    /// The `e_machine` field of an ELF header for this architecture.
    pub fn elf_machine(self) -> u16 { match self { Arch::X86_64 => 62 } }
}

impl Os {
    pub const ALL: [Os; 1] = [Os::Linux];
    pub fn name(self) -> &'static str { match self { Os::Linux => "linux" } }
    /// Executable file format used on this operating system.
    pub fn object_format(self) -> &'static str { match self { Os::Linux => "ELF" } }
}

impl Target {
    /// The only target implemented so far. Compilation is always a cross
    /// compilation in principle: it does not depend on the machine running `ccc`.
    pub const DEFAULT: Target = Target { arch: Arch::X86_64, os: Os::Linux };

    /// Every supported target, for error messages.
    pub fn supported() -> Vec<Target> {
        Arch::ALL.iter().flat_map(|a| Os::ALL.iter().map(move |o| Target { arch: *a, os: *o })).collect()
    }

    /// Accepts `ARCH-OS` optionally decorated like a full triple: `x86_64-linux`,
    /// `x86_64-linux-gnu`, `x86_64-unknown-linux-gnu`.
    pub fn parse(text: &str) -> Result<Target, String> {
        let supported = || Target::supported().iter().map(Target::to_string).collect::<Vec<_>>().join(", ");
        let parts: Vec<&str> = text.split('-').collect();
        let arch = *parts.first().filter(|p| !p.is_empty()).ok_or_else(|| format!("empty target '{text}'"))?;
        let arch = Arch::ALL.iter().copied().find(|a| a.name() == arch)
            .ok_or_else(|| format!("unsupported architecture '{arch}' in target '{text}' (supported targets: {})", supported()))?;
        // The operating system is the first known name after the architecture; a
        // vendor part (`unknown`, `pc`) and an environment (`gnu`) are tolerated.
        let rest = &parts[1..];
        let os = rest.iter().find_map(|p| Os::ALL.iter().copied().find(|o| o.name() == *p));
        match os {
            Some(os) => Ok(Target { arch, os }),
            None => {
                let named = rest.iter().find(|p| !matches!(**p, "unknown" | "pc" | "gnu" | "none")).copied().unwrap_or("");
                Err(format!("unsupported operating system '{named}' in target '{text}' (supported targets: {})", supported()))
            }
        }
    }

    /// Triple shown by `--version`, e.g. `x86_64-linux-gnu`.
    pub fn triple(&self) -> String {
        match self.os { Os::Linux => format!("{}-linux-gnu", self.arch.name()) }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}-{}", self.arch.name(), self.os.name()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn parses_short_and_decorated_triples() {
        for text in ["x86_64-linux", "x86_64-linux-gnu", "x86_64-unknown-linux-gnu", "x86_64-pc-linux-gnu"] {
            assert_eq!(Target::parse(text), Ok(Target::DEFAULT), "{text}");
        }
    }
    #[test] fn rejects_unknown_architecture_and_os_naming_the_alternatives() {
        let arch = Target::parse("aarch64-linux").unwrap_err();
        assert!(arch.contains("unsupported architecture 'aarch64'") && arch.contains("x86_64-linux"), "{arch}");
        let os = Target::parse("x86_64-windows").unwrap_err();
        assert!(os.contains("unsupported operating system 'windows'") && os.contains("x86_64-linux"), "{os}");
        assert!(Target::parse("x86_64").unwrap_err().contains("unsupported operating system"));
        assert!(Target::parse("").is_err());
    }
    #[test] fn display_and_triple() {
        assert_eq!(Target::DEFAULT.to_string(), "x86_64-linux");
        assert_eq!(Target::DEFAULT.triple(), "x86_64-linux-gnu");
        assert_eq!(Target::parse(&Target::DEFAULT.to_string()), Ok(Target::DEFAULT));
    }
    #[test] fn every_supported_target_round_trips() {
        for t in Target::supported() { assert_eq!(Target::parse(&t.to_string()), Ok(t)); }
    }
}
