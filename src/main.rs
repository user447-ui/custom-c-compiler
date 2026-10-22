mod ast; mod codegen; mod elf; mod ir; mod lexer; mod parser; mod regalloc; mod sema;
use std::{env,fs,process};
fn run()->Result<(),String>{
 let a:Vec<String>=env::args().collect();
 if a.len()==2&&a[1]=="--version"{println!("ccc 0.13.0 (x86_64-linux-gnu)");return Ok(())}
 if a.len()!=4||a[1]!="-o"{return Err("usage: ccc -o OUTPUT INPUT.c\n       ccc --version".into())}
 let source=fs::read_to_string(&a[3]).map_err(|e|format!("cannot read {}: {e}",a[3]))?;
 let tokens=lexer::lex(&source)?;
 let parsed=parser::Parser::new(tokens).parse()?;
 let program=sema::analyze(&parsed)?;
 let mut ir = ir::lower(&program);
 ir::promote_locals_to_ssa(&mut ir);
 let ir = ir::optimize(ir);
 let machine_code=codegen::generate(&ir)?;
 let binary=elf::executable(&machine_code);
 fs::write(&a[2],binary).map_err(|e|format!("cannot write {}: {e}",a[2]))?;
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;fs::set_permissions(&a[2],fs::Permissions::from_mode(0o755)).map_err(|e|format!("cannot set executable permissions: {e}"))?;}
 eprintln!("ccc: emitted native x86-64 ELF executable: {}",a[2]);Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("ccc: error: {e}");process::exit(1)}}
