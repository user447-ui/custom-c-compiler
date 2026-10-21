mod codegen;
mod elf;
mod lexer;
mod parser;

use std::{env, fs, process};

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && args[1] == "--version" {
        println!("ccc 0.2.0 (x86_64-linux-gnu)");
        return Ok(());
    }
    if args.len() != 4 || args[1] != "-o" {
        return Err("usage: ccc -o OUTPUT INPUT.c\n       ccc --version".into());
    }

    let source = fs::read_to_string(&args[3])
        .map_err(|e| format!("cannot read {}: {e}", args[3]))?;
    let tokens = lexer::lex(&source)?;
    let program = parser::Parser::new(tokens).parse()?;
    let machine_code = codegen::generate(&program.return_expr)?;
    let binary = elf::executable(&machine_code);
    fs::write(&args[2], binary)
        .map_err(|e| format!("cannot write {}: {e}", args[2]))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&args[2], fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("cannot set executable permissions: {e}"))?;
    }
    eprintln!("ccc: emitted native x86-64 ELF executable: {}", args[2]);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ccc: error: {error}");
        process::exit(1);
    }
}
