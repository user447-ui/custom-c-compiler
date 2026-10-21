use std::{fs, process::Command};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn compiles_and_runs_integer_expressions() {
    let base = std::env::temp_dir().join(format!("ccc-e2e-{}", std::process::id()));
    let source = base.with_extension("c");
    let output = base.with_extension("out");
    let cases = [
        ("2 + 3 * 4", 14),
        ("(20 - 8) / 3", 4),
        ("17 % 5", 2),
        ("8 - 3", 5),
        ("~0", 255),
        ("(6 | 1) ^ 3", 4),
        ("-7 + 10", 3),
    ];

    for (index, (expression, expected)) in cases.into_iter().enumerate() {
        fs::write(&source, format!("int main(void) {{ return {expression}; }}")).unwrap();
        let compiler = env!("CARGO_BIN_EXE_ccc");
        let compile = Command::new(compiler).arg("-o").arg(&output).arg(&source).output().unwrap();
        assert!(compile.status.success(), "case {index}: compiler stderr: {}", String::from_utf8_lossy(&compile.stderr));
        let run = Command::new(&output).status().unwrap();
        assert_eq!(run.code(), Some(expected), "case {index}: {expression}");
    }

    let _ = fs::remove_file(source);
    let _ = fs::remove_file(output);
}
