use std::{fs, process::Command};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn compiles_and_runs_arithmetic_expression() {
    let base = std::env::temp_dir().join(format!("ccc-e2e-{}", std::process::id()));
    let source = base.with_extension("c");
    let output = base.with_extension("out");
    fs::write(&source, "int main(void) { return 2 + 3 * 4; }").unwrap();

    let compiler = env!("CARGO_BIN_EXE_ccc");
    let compile = Command::new(compiler).arg("-o").arg(&output).arg(&source).output().unwrap();
    assert!(compile.status.success(), "compiler stderr: {}", String::from_utf8_lossy(&compile.stderr));

    let run = Command::new(&output).status().unwrap();
    assert_eq!(run.code(), Some(14));

    let _ = fs::remove_file(source);
    let _ = fs::remove_file(output);
}
