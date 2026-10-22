mod ast; mod backend; mod ir; mod irdump; mod lexer; mod object; mod parser; mod regalloc; mod sema; mod target;
use std::{env, fs, process};
use target::Target;

const USAGE: &str = "\
usage: ccc [options] INPUT.c

  -o OUTPUT            write a native executable
  --target=ARCH-OS     what to generate code for (default: x86_64-linux)
  --dump-ast           print the parsed AST
  --dump-ir[=STAGE]    print the IR; STAGE is one of
                         lowered  straight from the AST (locals in memory)
                         ssa      after promoting locals to SSA
                         opt      after optimization (the default)
  --dump-regalloc      like --dump-ir=opt, annotating every value with its
                       register or spill slot and live range
  --version
  -h, --help

At least one of -o or a --dump option is required. Dumps go to stdout.";

#[derive(Debug, Clone, Copy, PartialEq)]
enum IrStage { Lowered, Ssa, Optimized }

#[derive(Debug, PartialEq)]
struct Options {
    input: String,
    output: Option<String>,
    target: Target,
    dump_ast: bool,
    dump_ir: Option<IrStage>,
    dump_regalloc: bool,
}

#[derive(Debug, PartialEq)]
enum Command { Version, Help, Compile(Options) }

fn parse_args(args: &[String]) -> Result<Command, String> {
    let (mut input, mut output) = (None::<String>, None::<String>);
    let mut target = Target::DEFAULT;
    let (mut dump_ast, mut dump_ir, mut dump_regalloc) = (false, None, false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => return Ok(Command::Version),
            "-h" | "--help" => return Ok(Command::Help),
            "-o" => {
                let path = args.next().ok_or("option '-o' needs an output path")?;
                if output.replace(path.clone()).is_some() { return Err("option '-o' given more than once".into()); }
            }
            "--target" => target = Target::parse(args.next().ok_or("option '--target' needs a value")?)?,
            other if other.starts_with("--target=") => target = Target::parse(&other["--target=".len()..])?,
            "--dump-ast" => dump_ast = true,
            "--dump-ir" | "--dump-ir=opt" => dump_ir = Some(IrStage::Optimized),
            "--dump-ir=lowered" => dump_ir = Some(IrStage::Lowered),
            "--dump-ir=ssa" => dump_ir = Some(IrStage::Ssa),
            "--dump-regalloc" => dump_regalloc = true,
            other if other.starts_with("--dump-ir=") => {
                return Err(format!("unknown IR stage '{}' (expected lowered, ssa or opt)", &other["--dump-ir=".len()..]));
            }
            other if other.starts_with('-') && other.len() > 1 => return Err(format!("unknown option '{other}'")),
            other => {
                if input.replace(other.to_string()).is_some() { return Err("more than one input file given".into()); }
            }
        }
    }
    let input = input.ok_or("no input file")?;
    if dump_regalloc {
        match dump_ir {
            None | Some(IrStage::Optimized) => dump_ir = Some(IrStage::Optimized),
            Some(_) => return Err("--dump-regalloc only applies to the optimized IR (--dump-ir=opt)".into()),
        }
    }
    if output.is_none() && !dump_ast && dump_ir.is_none() {
        return Err("nothing to do: give -o OUTPUT or a --dump option".into());
    }
    Ok(Command::Compile(Options { input, output, target, dump_ast, dump_ir, dump_regalloc }))
}

/// `rbx  live [12, 40]` style comment for the value defined by an instruction.
fn allocation_comment(backend: &dyn backend::Backend, function: &ir::IrFunction, value: ir::ValueId) -> Option<String> {
    let allocation = regalloc::allocate(function, backend.allocatable_registers());
    let location = match allocation.locations.get(value)? {
        regalloc::Location::Register(r) => backend.register_name(*r).to_string(),
        regalloc::Location::Spill(slot) => format!("spill #{slot}"),
    };
    Some(match allocation.intervals.get(value).copied().flatten() {
        Some((start, end)) => format!("{location}  live [{start}, {end}]"),
        None => location,
    })
}

fn compile(options: &Options) -> Result<(), String> {
    let source = fs::read_to_string(&options.input).map_err(|e| format!("cannot read {}: {e}", options.input))?;
    let tokens = lexer::lex(&source)?;
    let parsed = parser::Parser::new(tokens).parse()?;
    if options.dump_ast { println!("{parsed:#?}"); }
    let program = sema::analyze(&parsed)?;
    let backend = backend::for_target(&options.target);
    let mut module = ir::lower(&program);
    let no_notes = |_: &ir::IrFunction, _: ir::ValueId| None;
    if options.dump_ir == Some(IrStage::Lowered) { print!("{}", irdump::module_to_string(&module, &no_notes)); }
    ir::promote_locals_to_ssa(&mut module);
    if options.dump_ir == Some(IrStage::Ssa) { print!("{}", irdump::module_to_string(&module, &no_notes)); }
    let module = ir::optimize(module);
    if options.dump_ir == Some(IrStage::Optimized) {
        if options.dump_regalloc {
            print!("{}", irdump::module_to_string(&module, &|f, v| allocation_comment(&*backend, f, v)));
        } else {
            print!("{}", irdump::module_to_string(&module, &no_notes));
        }
    }
    let Some(output) = &options.output else { return Ok(()) };
    let machine_code = backend::generate(&*backend, options.target.os, &module)?;
    let binary = object::executable(&options.target, &machine_code)?;
    fs::write(output, binary).map_err(|e| format!("cannot write {output}: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(output, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("cannot set executable permissions: {e}"))?;
    }
    eprintln!("ccc: emitted native {} {} executable: {output}", options.target.arch.pretty(), options.target.os.object_format());
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_args(&args).map_err(|e| format!("{e}\n{USAGE}"))? {
        Command::Version => { println!("ccc {} ({})", env!("CARGO_PKG_VERSION"), Target::DEFAULT.triple()); Ok(()) }
        Command::Help => { println!("{USAGE}"); Ok(()) }
        Command::Compile(options) => compile(&options),
    }
}

fn main() {
    if let Err(e) = run() { eprintln!("ccc: error: {e}"); process::exit(1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    fn compile_options(args: &[&str]) -> Options {
        match parse(args).unwrap() { Command::Compile(o) => o, other => panic!("{other:?}") }
    }

    #[test] fn classic_invocation_still_works() {
        let o = compile_options(&["-o", "out", "in.c"]);
        assert_eq!((o.input.as_str(), o.output.as_deref(), o.dump_ir, o.dump_ast), ("in.c", Some("out"), None, false));
    }
    #[test] fn options_may_come_in_any_order() {
        assert_eq!(compile_options(&["in.c", "-o", "out"]), compile_options(&["-o", "out", "in.c"]));
    }
    #[test] fn dump_ir_stages() {
        assert_eq!(compile_options(&["--dump-ir", "a.c"]).dump_ir, Some(IrStage::Optimized));
        assert_eq!(compile_options(&["--dump-ir=opt", "a.c"]).dump_ir, Some(IrStage::Optimized));
        assert_eq!(compile_options(&["--dump-ir=lowered", "a.c"]).dump_ir, Some(IrStage::Lowered));
        assert_eq!(compile_options(&["--dump-ir=ssa", "a.c"]).dump_ir, Some(IrStage::Ssa));
        assert!(parse(&["--dump-ir=bogus", "a.c"]).unwrap_err().contains("unknown IR stage 'bogus'"));
    }
    #[test] fn dump_regalloc_implies_optimized_ir() {
        let o = compile_options(&["--dump-regalloc", "a.c"]);
        assert!(o.dump_regalloc && o.dump_ir == Some(IrStage::Optimized));
        assert!(parse(&["--dump-regalloc", "--dump-ir=ssa", "a.c"]).is_err());
    }
    #[test] fn dumps_and_output_combine() {
        let o = compile_options(&["--dump-ast", "--dump-ir=ssa", "-o", "x", "a.c"]);
        assert!(o.dump_ast && o.dump_ir == Some(IrStage::Ssa) && o.output.as_deref() == Some("x"));
    }
    #[test] fn rejects_bad_command_lines() {
        assert!(parse(&["a.c"]).unwrap_err().contains("nothing to do"));
        assert!(parse(&["-o", "x"]).unwrap_err().contains("no input file"));
        assert!(parse(&["-o"]).unwrap_err().contains("needs an output path"));
        assert!(parse(&["--frobnicate", "a.c"]).unwrap_err().contains("unknown option"));
        assert!(parse(&["a.c", "b.c", "-o", "x"]).unwrap_err().contains("more than one input"));
        assert!(parse(&["-o", "x", "-o", "y", "a.c"]).unwrap_err().contains("more than once"));
    }
    #[test] fn target_option_both_spellings() {
        assert_eq!(compile_options(&["--target=x86_64-linux-gnu", "-o", "x", "a.c"]).target, Target::DEFAULT);
        assert_eq!(compile_options(&["--target", "x86_64-linux", "-o", "x", "a.c"]).target, Target::DEFAULT);
        assert_eq!(compile_options(&["-o", "x", "a.c"]).target, Target::DEFAULT);
        assert!(parse(&["--target=aarch64-linux", "-o", "x", "a.c"]).unwrap_err().contains("unsupported architecture 'aarch64'"));
        assert!(parse(&["--target=x86_64-windows", "-o", "x", "a.c"]).unwrap_err().contains("unsupported operating system 'windows'"));
        assert!(parse(&["a.c", "--target"]).unwrap_err().contains("needs a value"));
    }
    #[test] fn version_and_help() {
        assert_eq!(parse(&["--version"]).unwrap(), Command::Version);
        assert_eq!(parse(&["-h"]).unwrap(), Command::Help);
    }
}
