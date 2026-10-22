use std::{fs,process::Command};
#[cfg(all(target_os="linux",target_arch="x86_64"))]
#[test]
fn compiles_and_runs_expressions_and_statements(){
 let base=std::env::temp_dir().join(format!("ccc-e2e-{}",std::process::id()));let src=base.with_extension("c");let out=base.with_extension("out");
 let cases=[
 ("return 2+3*4;",14),("return (20-8)/3;",4),("return 17%5;",2),
 ("int x=10;x=x+7;return x;",17),("int x=0;if(1)x=7;else x=8;return x;",7),
 ("int x=0;if(0)x=7;else x=8;return x;",8),("int x=0;while(x-5)x=x+1;return x;",5),
 ("int x=1;if(x<2)x=x+3;else x=x+4;return x;",4),
 ("int x=0;while(x<5)x=x+1;return x;",5),
 ("int x=3;{int y=4;x=x+y;}return x;",7),("int x=1;int y=2;while(y-6){x=x*2;y=y+1;}return x;",16),
 ("if(2<3)return 11;return 22;",11),("if(3<=3)return 12;return 22;",12),
 ("if(4>3)return 13;return 22;",13),("if(4>=5)return 22;return 14;",14),
 ("if(4==4)return 15;return 22;",15),("if(4!=4)return 22;return 16;",16),
 ("if(!0)return 17;return 22;",17),("if(0||7)return 18;return 22;",18),
 ("if(1&&2)return 19;return 22;",19),("return 1+2*3==7;",1),
 ("return 0||0;",0),("return 4>=3 && 2!=1;",1),("return 0 || 1 && 0;",0),
 ("int x=3;{int x=7;x=x+1;}return x;",3),
 ("return 0 && (1/0);",0),("return 1 || (1/0);",1),
 ("return 1 && (2<3);",1),("return 0 || (2<3);",1),
 ("int x;if(1)x=7;else x=8;return x;",7),
 ("int a=5;int i=0;int s=0;while(i<4){s=s+a;int t=i*3;int u=t+1;i=i+1;s=s+u-u;}return s;",20),
 ("int x=3;int y=x*2;int i=0;int acc=0;while(i<3){acc=acc+y;int p=acc+1;int q=p+2;acc=q-3;i=i+1;}return acc;",18),
 ("int a=8;int b=6;int c=16;return a && ((b || c) > (a != 10));",0),
 ("return 1 && (0 || (1 && 0));",0),("return 0 || (1 && (0 || 1));",1),
 ];
 for(i,(body,want))in cases.into_iter().enumerate(){fs::write(&src,format!("int main(void){{{body}}}")).unwrap();let c=Command::new(env!("CARGO_BIN_EXE_ccc")).arg("-o").arg(&out).arg(&src).output().unwrap();assert!(c.status.success(),"case {i}: {}",String::from_utf8_lossy(&c.stderr));let run=Command::new(&out).status().unwrap();assert_eq!(run.code(),Some(want),"case {i}: {body}");}
 let _=fs::remove_file(src);let _=fs::remove_file(out);
}
#[cfg(all(target_os="linux",target_arch="x86_64"))]
#[test]
fn rejects_invalid_names_and_uninitialized_reads(){for (i,source) in [
 "int main(){return missing;}",
 "int main(){int x; int x; return 0;}",
 "int main(){x=2; int x; return 0;}",
 "int main(){int x; return x;}",
 "int main(){int x;if(1)x=3;return x;}",
 "int main(){int x;while(0)x=4;return x;}",
].into_iter().enumerate(){let p=std::env::temp_dir().join(format!("ccc-bad-{}-{i}.c",std::process::id()));fs::write(&p,source).unwrap();let out=p.with_extension("out");let c=Command::new(env!("CARGO_BIN_EXE_ccc")).arg("-o").arg(&out).arg(&p).output().unwrap();assert!(!c.status.success(),"accepted invalid source: {source}");let _=fs::remove_file(p);let _=fs::remove_file(out);}}

// ---------------------------------------------------------------------------
// Whole-program tests: multiple functions, calls and the System V calling convention.
// Expected exit codes were cross-checked against gcc.
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn compile_and_run(tag: &str, source: &str) -> Result<i32, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let base = std::env::temp_dir().join(format!("ccc-prog-{}-{}-{tag}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
    let (src, out) = (base.with_extension("c"), base.with_extension("out"));
    fs::write(&src, source).unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_ccc")).arg("-o").arg(&out).arg(&src).output().unwrap();
    let result = if compiled.status.success() {
        Ok(Command::new(&out).status().unwrap().code().expect("terminated by a signal"))
    } else {
        Err(String::from_utf8_lossy(&compiled.stderr).into_owned())
    };
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
    result
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn runs_programs_with_functions_and_calls() {
    let programs: [(&str, &str, i32); 17] = [
        ("add", "int add(int a,int b){return a+b;} int main(void){return add(40,2);}", 42),
        ("factorial", "int fact(int n){ if(n<=1) return 1; return n*fact(n-1);} int main(void){return fact(5);}", 120),
        ("fibonacci", "int fib(int n){ if(n<2) return n; return fib(n-1)+fib(n-2);} int main(void){return fib(15);}", 98),
        ("gcd", "int gcd(int a,int b){ while(b){int t=b; b=a%b; a=t;} return a;} int main(void){return gcd(84,36);}", 12),
        ("mutual recursion via prototype",
         "int odd(int n); int even(int n){ if(n==0) return 1; return odd(n-1);} int odd(int n){ if(n==0) return 0; return even(n-1);} int main(void){return even(10)*10+odd(7);}", 11),
        ("six register args", "int f(int a,int b,int c,int d,int e,int g){return a+2*b+3*c+4*d+5*e+6*g;} int main(void){return f(1,2,3,4,5,6);}", 91),
        ("seven args (one on the stack)", "int f(int a,int b,int c,int d,int e,int g,int h){return a+2*b+3*c+4*d+5*e+6*g+7*h;} int main(void){return f(1,2,3,4,5,6,7);}", 140),
        ("eight args (two on the stack)", "int f(int a,int b,int c,int d,int e,int g,int h,int i){return a-b+c-d+e-g+h*i;} int main(void){return f(1,2,3,4,5,6,7,8);}", 53),
        ("ten args", "int f(int a,int b,int c,int d,int e,int g,int h,int i,int j,int k){return a+b*2+c*3+d*4+e*5+g*6+h*7+i*8+j*9+k*10;} int main(void){return f(1,2,3,4,5,6,7,8,9,10)%200;}", 185),
        ("nested calls as arguments", "int g(int x){return x*2;} int f(int a,int b){return a+b;} int main(void){return f(g(3),g(f(1,2)));}", 12),
        ("registers survive calls",
         "int id(int x){return x;} int main(void){int a=1;int b=2;int c=3;int d=4;int e=5;int s=0;int i=0; while(i<3){ s=s+id(a)+id(b)*2+id(c)*3+id(d)*4+id(e)*5; i=i+1;} return s+a+b+c+d+e;}", 180),
        ("call statements", "int f(int x){return x+1;} int main(void){f(1);f(2);return f(3);}", 4),
        ("no arguments", "int seven(void){return 7;} int main(void){return seven()+seven();}", 14),
        ("unused leading params", "int f(int a,int b,int c){return c;} int main(void){return f(1,2,9);}", 9),
        ("deep recursion", "int f(int n){ if(n==0) return 0; return 1+f(n-1);} int main(void){return f(1000)%256;}", 232),
        ("params are copies", "int f(int a){ a=a+10; return a;} int main(void){int x=5; int y=f(x); return x*10+y%10;}", 55),
        ("calls in short-circuit operands", "int p(int x){return x>2;} int main(void){int r=0; if(p(3)&&p(1+1)) r=1; if(p(5)||p(0)) r=r+2; return r;}", 2),
    ];
    for (name, source, want) in programs {
        assert_eq!(compile_and_run("ok", source), Ok(want), "program '{name}': {source}");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn rejects_invalid_function_programs() {
    for (source, expected) in [
        ("int main(){return f();}", "undeclared function"),
        ("int main(){return f();} int f(void){return 1;}", "undeclared function"),
        ("int f(int a){return a;} int main(){return f(1,2);}", "expects 1"),
        ("int f(void){return 1;}", "no 'main'"),
        ("int main(int argc){return 0;}", "no parameters"),
        ("int f(void){return 1;} int f(void){return 2;} int main(){return 0;}", "redefinition"),
        ("int f(int a); int f(int a,int b){return 0;} int main(){return 0;}", "conflicting"),
        ("int f(void); int main(){return f();}", "never defined"),
        ("int f(int a,int a){return a;} int main(){return 0;}", "duplicate parameter"),
        ("int f(int a){int a; return a;} int main(){return 0;}", "redeclaration"),
        ("int main(){int x; return f(x);}", "undeclared function"),
        ("int f(int a){return a;} int main(){int x; return f(x);}", "uninitialized"),
    ] {
        match compile_and_run("bad", source) {
            Err(message) => assert!(message.contains(expected), "wrong diagnostic for {source}: {message}"),
            Ok(code) => panic!("accepted invalid program (exit {code}): {source}"),
        }
    }
}
