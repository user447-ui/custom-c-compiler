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

// ---------------------------------------------------------------------------
// `int` is 32 bits wide and arithmetic wraps. Each operation and its comparison
// live in the same function (a call boundary re-normalises the result, which
// would hide a missing wrap), and operands arrive through parameters so the
// optimizer cannot fold them. Every case is true under `gcc -fwrapv`.
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn int_is_32_bits_and_wraps() {
    let programs: [(&str, &str, i32); 26] = [
        ("negative constant on the right of a comparison", "int gt_m1(int a){return a > -1;} int main(void){ return gt_m1(5); }", 1),
        ("negative constant on the left of a comparison", "int lt_m1(int a){return -1 < a;} int main(void){ return lt_m1(5); }", 1),
        ("negative constant in arithmetic", "int f(int a){return a + -3 == 2;} int main(void){ return f(5); }", 1),
        ("negative constant is below zero", "int f(int a){return -100 < a && a < 100;} int main(void){ return f(0); }", 1),
        ("add wraps to negative", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return add_lt0(2147483647,1); }", 1),
        ("add max+max", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return add_eq(2147483647,2147483647,-2); }", 1),
        ("sub wraps to positive", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return sub_gt0((-2147483647-1),1); }", 1),
        ("sub min-max", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return sub_eq((-2147483647-1),2147483647,1); }", 1),
        ("mul 65536*65536", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return mul_eq(65536,65536,0); }", 1),
        ("mul 46341*46341 is negative", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return mul_lt0(46341,46341); }", 1),
        ("mul 123456789*37", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return mul_eq(123456789,37,272933897); }", 1),
        ("negating INT_MIN stays negative", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return neg_lt0((-2147483647-1)); }", 1),
        ("negating INT_MIN is INT_MIN", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return neg_eq((-2147483647-1),(-2147483647-1)); }", 1),
        ("division of INT_MIN", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return quot_eq((-2147483647-1),7,-306783378); }", 1),
        ("remainder of INT_MIN", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return mod_eq((-2147483647-1),7,-2); }", 1),
        ("division truncates toward zero", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return quot_eq(-7,2,-3); }", 1),
        ("remainder takes dividend sign", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return mod_eq(-7,3,-1); }", 1),
        ("overflowed result crosses a call", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return is_neg(add(2147483647,1)); }", 1),
        ("overflow inside a condition", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ int a=2147483647; if (add(a,1) < a) return 1; return 0; }", 1),
        ("fold: INT_MAX+1", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return (2147483647 + 1) < 0; }", 1),
        ("fold: 65536*65536", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return 65536 * 65536 == 0; }", 1),
        ("fold: INT_MIN literal", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return -2147483648 < 0; }", 1),
        ("fold: negate INT_MIN", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ return -(-2147483647 - 1) < 0; }", 1),
        ("bitnot of INT_MAX", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ int a = 2147483647; return (~a) < 0 && (a + 1) == (~a); }", 1),
        ("seventh (stack) argument", "int f(int a,int b,int c,int d,int e,int g,int h){return (h+a) < 0;} int main(void){ return f(2147483647,0,0,0,0,0,1); }", 1),
        ("hash loop wraps", "int add_lt0(int a,int b){return a+b<0;} int add_eq(int a,int b,int c){return a+b==c;} int sub_gt0(int a,int b){return a-b>0;} int sub_eq(int a,int b,int c){return a-b==c;} int mul_lt0(int a,int b){return a*b<0;} int mul_eq(int a,int b,int c){return a*b==c;} int neg_lt0(int a){return -a<0;} int neg_eq(int a,int c){return -a==c;} int quot_eq(int a,int b,int c){return a/b==c;} int mod_eq(int a,int b,int c){return a%b==c;} int is_neg(int x){return x<0;} int add(int a,int b){return a+b;} int main(void){ int h=5381; int i=0; while(i<20){ h=h*33+i; i=i+1; } return (h % 251) + 100; }", 33),
    ];
    for (name, source, want) in programs {
        assert_eq!(compile_and_run("int32", source), Ok(want), "program '{name}': {source}");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn rejects_integer_literals_that_do_not_fit_in_int() {
    assert_eq!(compile_and_run("lit", "int main(void){ return 2147483647 == 2147483647; }"), Ok(1));
    for source in ["int main(void){ return 2147483648; }", "int main(void){ return 4294967296 == 0; }", "int main(void){ return -2147483649; }"] {
        let message = compile_and_run("lit", source).expect_err(source);
        assert!(message.contains("too large for 'int'"), "{source}: {message}");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn octal_and_hexadecimal_literals_match_c() {
    let programs = [
        ("octal is base 8", "int main(void){ return 010; }", 8),
        ("octal 0777", "int main(void){ return 0777 == 511; }", 1),
        ("zero", "int main(void){ return 0 + 00 + 0x0; }", 0),
        ("hex", "int main(void){ return 0x2A; }", 42),
        ("hex upper-case digits and prefix", "int main(void){ return 0XfF == 255; }", 1),
        ("INT_MAX in hex", "int main(void){ return 0x7fffffff == 2147483647; }", 1),
        ("INT_MAX in octal", "int main(void){ return 017777777777 == 2147483647; }", 1),
        ("negated hex", "int main(void){ return -0x10 == -16; }", 1),
        ("in expressions", "int main(void){ int a = 0x10; int b = 010; return a - b; }", 8),
    ];
    for (name, source, want) in programs {
        assert_eq!(compile_and_run("radix", source), Ok(want), "program '{name}': {source}");
    }
    let rejected = [
        ("int main(void){ return 08; }", "invalid digit '8' in octal"),
        ("int main(void){ return 0x; }", "no digits"),
        ("int main(void){ return 0xg; }", "invalid suffix"),
        ("int main(void){ return 12abc; }", "invalid suffix"),
        ("int main(void){ return 0x80000000; }", "does not fit in 'int'"),
        ("int main(void){ return 0xffffffff; }", "does not fit in 'int'"),
        ("int main(void){ return -0x80000000; }", "does not fit in 'int'"),
    ];
    for (source, expected) in rejected {
        let message = compile_and_run("radix", source).expect_err(source);
        assert!(message.contains(expected), "{source}: {message}");
    }
}

/// The limits these tests probe live in `src/parser.rs` (a binary crate, so they are
/// repeated here): 256 levels of parentheses/unary/call nesting, 1000 levels of statement
/// nesting, and expression trees up to 10000 levels tall.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn programs_at_the_nesting_limits_compile_and_run() {
    let nested_parens = format!("int main(void){{ return {}7{}; }}", "(".repeat(256), ")".repeat(256));
    assert_eq!(compile_and_run("deep", &nested_parens), Ok(7));
    let blocks = format!("int main(void){{ {}return 9;{} }}", "{".repeat(999), "}".repeat(999));
    assert_eq!(compile_and_run("deep", &blocks), Ok(9));
    let mut rungs = String::new();
    for i in 0..998 { rungs.push_str(&format!("if (x == {i}) return {}; else ", i % 200 + 1)); }
    let ladder = format!("int f(int x){{ {rungs}return 0; }} int main(void){{ return f(997); }}");
    assert_eq!(compile_and_run("deep", &ladder), Ok(997 % 200 + 1)); // rung 997 returns 198
    // 10000 terms: the tallest expression tree that is accepted. 10000 % 256 = 16.
    let chain = format!("int main(void){{ return {}; }}", vec!["1"; 10_000].join("+"));
    assert_eq!(compile_and_run("deep", &chain), Ok(16));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn programs_beyond_the_nesting_limits_are_rejected_not_crashed() {
    let chain = |n: usize| vec!["1"; n].join("+");
    let cases = [
        (format!("int main(void){{ return {}1{}; }}", "(".repeat(257), ")".repeat(257)), "expressions nested too deeply"),
        (format!("int main(void){{ return {}1; }}", "-".repeat(100_000)), "expressions nested too deeply"),
        (format!("int f(int x){{ return x; }} int main(void){{ return {}1{}; }}", "f(".repeat(10_000), ")".repeat(10_000)), "expressions nested too deeply"),
        (format!("int main(void){{ return {}1{}; }}", "(".repeat(1_000_000), ")".repeat(1_000_000)), "expressions nested too deeply"),
        (format!("int main(void){{ {}return 1;{} }}", "{".repeat(1000), "}".repeat(1000)), "statements nested too deeply"),
        (format!("int main(void){{ {}return 0; }}", "if (1) ".repeat(100_000)), "statements nested too deeply"),
        (format!("int main(void){{ {}; return 0; }}", "while (0) ".repeat(50_000)), "statements nested too deeply"),
        (format!("int main(void){{ return {}; }}", chain(10_001)), "expression too complex"),
        (format!("int main(void){{ return {}; }}", chain(1_000_000)), "expression too complex"),
    ];
    for (i, (source, expected)) in cases.iter().enumerate() {
        let p = temp_source(&format!("limit{i}"), source);
        let out = p.with_extension("out");
        let c = Command::new(env!("CARGO_BIN_EXE_ccc")).arg("-o").arg(&out).arg(&p).output().unwrap();
        let stderr = String::from_utf8_lossy(&c.stderr);
        // A clean diagnostic means exit status 1; a stack overflow would be a signal (no code).
        assert_eq!(c.status.code(), Some(1), "case {i}: {}", &stderr[..stderr.len().min(300)]);
        assert!(stderr.contains(expected), "case {i}: {}", &stderr[..stderr.len().min(300)]);
        assert!(!out.exists(), "case {i}: an executable was written for a rejected program");
        let _ = fs::remove_file(p);
    }
}

/// Big functions must not need memory (or time) quadratic in their size. 6000 locals and
/// 6000 `if`s make about 18000 blocks: per-block dominator sets or per-block snapshots of every
/// variable needed gigabytes, so the compiler runs under a 900 MB address-space limit.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn large_functions_compile_in_bounded_memory() {
    let n = 6000;
    let mut source = String::from("int main(void){ int x = 1; int y = 0; ");
    let mut expected = 0i64;
    for i in 0..n {
        source.push_str(&format!("int a{i} = x + {i}; if (a{i} % 3) y = y + a{i}; "));
        if (1 + i) % 3 != 0 { expected += 1 + i; }
    }
    source.push_str("return y % 251; }");
    let src = temp_source("bigfn", &source);
    let out = src.with_extension("out");
    let compiled = Command::new("sh").arg("-c").arg("ulimit -v 900000 && exec \"$0\" \"$@\"")
        .arg(env!("CARGO_BIN_EXE_ccc")).arg("-o").arg(&out).arg(&src).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    assert_eq!(Command::new(&out).status().unwrap().code(), Some((expected % 251) as i32));
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
}

/// Compile `source` with `flags`; returns (exit code, stderr, whether the output file exists)
/// and, when `run` is set and the compile worked, how the built program ended.
fn compile_with_flags(tag: &str, source: &str, flags: &[&str], run: bool) -> (Option<i32>, String, bool, Option<std::process::ExitStatus>) {
    let src = temp_source(tag, source);
    let out = src.with_extension("out");
    let compiled = Command::new(env!("CARGO_BIN_EXE_ccc")).args(flags).arg("-o").arg(&out).arg(&src).output().unwrap();
    let exists = out.exists();
    let ended = (run && exists).then(|| Command::new(&out).status().unwrap());
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
    (compiled.status.code(), String::from_utf8_lossy(&compiled.stderr).into_owned(), exists, ended)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn certain_division_faults_are_warned_about() {
    use std::os::unix::process::ExitStatusExt;
    let cases = [
        ("literal", "int main(void){ return 1 / 0; }", "integer division by zero"),
        ("remainder", "int main(void){ return 7 % 0; }", "integer remainder by zero"),
        ("folded", "int main(void){ return 9 / (4 - 4); }", "integer division by zero"),
        ("variable", "int main(void){ int z = 0; return 10 / z; }", "integer division by zero"),
        ("other function", "int f(int a){ return a % 0; } int main(void){ return f(1); }", "in function 'f'"),
        ("int min", "int main(void){ return (-2147483647 - 1) / -1; }", "INT_MIN / -1 overflows"),
    ];
    for (name, source, expected) in cases {
        // A warning only: the program is still built, and it does fault (SIGFPE) when run.
        let (code, stderr, built, ended) = compile_with_flags("divwarn", source, &[], true);
        assert_eq!(code, Some(0), "{name}: {stderr}");
        assert!(built, "{name}: no executable was written");
        assert!(stderr.contains(": warning: ") && stderr.contains(expected), "{name}: {stderr}");
        assert_eq!(ended.unwrap().signal(), Some(8), "{name}: expected SIGFPE");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn werror_turns_the_warning_into_a_failure_and_clean_code_is_silent() {
    let bad = "int main(void){ int z = 0; return 1 / z; }";
    let (code, stderr, built, _) = compile_with_flags("werror", bad, &["-Werror"], false);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("warning: ") && stderr.contains("1 warning treated as an error (-Werror)"), "{stderr}");
    assert!(!built, "-Werror must not leave an executable behind");
    let good = "int f(int a, int b){ return a / b + a % b; } int main(void){ int d = 0; if (d != 0) { return 10 / d; } return f(7, 2); }";
    let (code, stderr, built, _) = compile_with_flags("werror", good, &["-Werror"], false);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(built && !stderr.contains("warning"), "{stderr}");
}

/// What `ccc` prints for `source`, with the temporary file's path replaced by `t.c`.
#[cfg(unix)]
fn compiler_output(tag: &str, source: &str, flags: &[&str]) -> (Option<i32>, String) {
    let src = temp_source(tag, source);
    let out = src.with_extension("out");
    let run = Command::new(env!("CARGO_BIN_EXE_ccc")).args(flags).arg("-o").arg(&out).arg(&src).output().unwrap();
    let stderr = String::from_utf8_lossy(&run.stderr).replace(src.to_str().unwrap(), "t.c").replace(out.to_str().unwrap(), "t.out");
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
    (run.status.code(), stderr)
}

#[cfg(unix)]
#[test]
fn errors_show_file_line_column_the_source_line_and_a_caret() {
    let cases: [(&str, &str); 9] = [
        ("int main(void) {\n    return 1 +;\n}\n",
         "t.c:2:15: error: expected expression, got ';'\n    return 1 +;\n              ^\n"),
        // a missing `;` is shown after the last token of the statement, not on the next line
        ("int main(void) {\n    int x = 5\n    return x;\n}\n",
         "t.c:2:14: error: expected ';', got 'return'\n    int x = 5\n             ^\n"),
        ("int main(void) {\n    return y;\n}\n",
         "t.c:2:12: error: use of undeclared variable 'y'\n    return y;\n           ^\n"),
        ("int main(void) {\n    int a = 1;\n    int a = 2;\n    return a;\n}\n",
         "t.c:3:9: error: redeclaration of local variable 'a' in the same scope\n    int a = 2;\n        ^\n"),
        // tabs are copied into the caret line, so it lines up whatever the tab width
        ("int main(void) {\n\tint x;\n\treturn x + 1;\n}\n",
         "t.c:3:9: error: read of uninitialized variable 'x'\n\treturn x + 1;\n\t       ^\n"),
        ("int f(int a);\nint main(void) {\n    return f(1, 2);\n}\n",
         "t.c:3:12: error: function 'f' expects 1 argument(s), got 2\n    return f(1, 2);\n           ^\n"),
        ("int main(void) {\n    return 08;\n}\n",
         "t.c:2:12: error: invalid digit '8' in octal literal '08'\n    return 08;\n           ^\n"),
        // the unclosed block is shown where it opens, not at the end of the file
        ("int main(void) {\n    if (1) {\n        return 0;\n",
         "t.c:2:12: error: unterminated block: this '{' is never closed before the end of input\n    if (1) {\n           ^\n"),
        // an error about the program as a whole has no location
        ("int f(void) { return 0; }\n", "ccc: error: program has no 'main' function\n"),
    ];
    for (i, (source, expected)) in cases.iter().enumerate() {
        let (code, stderr) = compiler_output("diag", source, &[]);
        assert_eq!(code, Some(1), "case {i}");
        assert_eq!(&stderr, expected, "case {i}");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn warnings_show_the_location_of_the_operator() {
    let source = "int main(void) {\n    int z = 0;\n    return 10 / z;\n}\n";
    let (code, stderr) = compiler_output("warnpos", source, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stderr, "t.c:3:15: warning: in function 'main': integer division by zero; the program crashes if this runs\n    return 10 / z;\n              ^\nccc: emitted native x86-64 ELF executable: t.out\n");
    // -Werror: the same warning, then the failure
    let (code, stderr) = compiler_output("warnpos", source, &["-Werror"]);
    assert_eq!(code, Some(1));
    assert!(stderr.starts_with("t.c:3:15: warning: ") && stderr.ends_with("ccc: error: 1 warning treated as an error (-Werror)\n"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn input_with_windows_line_endings_and_non_ascii_text_is_located_correctly() {
    // CRLF line endings, and a multi-byte character before the error: the column counts
    // characters, and the shown line has no stray carriage return.
    let source = "int main(void) {\r\n    /* é */ return q;\r\n}\r\n";
    let (_, stderr) = compiler_output("crlf", source, &[]);
    assert_eq!(stderr, "t.c:2:20: error: use of undeclared variable 'q'\n    /* é */ return q;\n                   ^\n");
}

#[test]
fn dump_ast_shows_positions() {
    let src = temp_source("astpos", "int main(void) {\n  return 6 / 3;\n}\n");
    let (ok, text, _) = run_ccc(&["--dump-ast", src.to_str().unwrap()]);
    assert!(ok, "{text}");
    assert!(text.contains("pos: 1:5") && text.contains("pos: 2:12"), "{text}"); // main; the `/`
    let _ = fs::remove_file(src);
}

/// Register allocation must not need memory quadratic in (values live across blocks) x
/// (blocks): 3000 values that stay live through 3000 branches (about 9000 blocks) took over
/// a gigabyte as per-block sets of live values, so run the compiler under a 600 MB limit.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn many_long_lived_values_over_many_branches_compile_in_bounded_memory() {
    let n = 3000;
    let mut source = String::from("int main(void){ int y = 0; ");
    for i in 0..n { source.push_str(&format!("int a{i} = {i} + y; ")); }
    for _ in 0..n { source.push_str("if (y < 1) { y = y + 1; } "); }
    let sum = (0..n).map(|i| format!("a{i}")).collect::<Vec<_>>().join("+");
    source.push_str(&format!("return ({sum}) % 251; }}"));
    let src = temp_source("longlive", &source);
    let out = src.with_extension("out");
    let compiled = Command::new("sh").arg("-c").arg("ulimit -v 600000 && exec \"$0\" \"$@\"")
        .arg(env!("CARGO_BIN_EXE_ccc")).arg("-o").arg(&out).arg(&src).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let expected = (0..n).sum::<i64>() % 251;
    assert_eq!(Command::new(&out).status().unwrap().code(), Some(expected as i32));
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn unused_variables_and_useless_statements_are_warned_about_in_source_order() {
    let source = "int twice(int n) {\n    int spare = 0;\n    return n * 2;\n}\n\nint main(void) {\n    int z = 0;\n    int total;\n    total = 1;\n    twice(3);\n    z + 1;\n    return 5 / z;\n}\n";
    let (code, stderr) = compiler_output("lint", source, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    // AST warnings and the IR warning (the division) come out together, ordered by position
    assert_eq!(stderr, "\
t.c:2:9: warning: unused variable 'spare'\n    int spare = 0;\n        ^\n\
t.c:8:9: warning: variable 'total' is set but never used\n    int total;\n        ^\n\
t.c:11:7: warning: statement has no effect\n    z + 1;\n      ^\n\
t.c:12:14: warning: in function 'main': integer division by zero; the program crashes if this runs\n    return 5 / z;\n             ^\n\
ccc: emitted native x86-64 ELF executable: t.out\n");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn warnings_can_be_silenced_or_made_fatal() {
    let source = "int main(void) {\n    int a = 1;\n    int b = 2;\n    b;\n    return a;\n}\n";
    // -w: nothing but the usual note
    let (code, stderr) = compiler_output("lintw", source, &["-w"]);
    assert_eq!((code, stderr.as_str()), (Some(0), "ccc: emitted native x86-64 ELF executable: t.out\n"));
    // -Werror counts every warning (here two: `b;` has no effect, and b is only "used" by it)
    let (code, stderr) = compiler_output("lintw", source, &["-Werror"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("warning: statement has no effect") && stderr.ends_with("ccc: error: 1 warning treated as an error (-Werror)\n"), "{stderr}");
    // -w wins: with no warnings, -Werror has nothing to object to
    let (code, stderr) = compiler_output("lintw", source, &["-Werror", "-w"]);
    assert_eq!((code, stderr.as_str()), (Some(0), "ccc: emitted native x86-64 ELF executable: t.out\n"));
    // a program with nothing to warn about is silent
    let clean = "int gcd(int a, int b) {\n    while (b) {\n        int t = b;\n        b = a % b;\n        a = t;\n    }\n    return a;\n}\nint main(void) { return gcd(12, 18); }\n";
    let (code, stderr) = compiler_output("lintw", clean, &["-Werror"]);
    assert_eq!((code, stderr.as_str()), (Some(0), "ccc: emitted native x86-64 ELF executable: t.out\n"));
}

// ---------------------------------------------------------------------------
// Command line: --dump-ast / --dump-ir / --dump-regalloc
// ---------------------------------------------------------------------------

fn run_ccc(args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_ccc")).args(args).output().unwrap();
    (o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

fn temp_source(tag: &str, source: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ccc-dump-{}-{tag}.c", std::process::id()));
    fs::write(&p, source).unwrap();
    p
}

const GCD: &str = "int gcd(int a,int b){ while(b){int t=b; b=a%b; a=t;} return a;} int main(void){return gcd(84,36);}";

#[test]
fn dump_ir_stages_show_the_pipeline() {
    let src = temp_source("stages", GCD);
    let path = src.to_str().unwrap();
    let (ok, lowered, _) = run_ccc(&["--dump-ir=lowered", path]);
    assert!(ok && lowered.contains("function gcd(2 params)") && lowered.contains("= load b#1") && !lowered.contains("phi"), "{lowered}");
    let (ok, ssa, _) = run_ccc(&["--dump-ir=ssa", path]);
    assert!(ok && ssa.contains("= phi [bb0: %") && !ssa.contains("load"), "{ssa}");
    let (ok, opt, _) = run_ccc(&["--dump-ir", path]);
    assert!(ok && opt.contains("= srem") && opt.contains("= call gcd(%0, %1)") && opt.contains("ret %"), "{opt}");
    assert!(opt.matches("function ").count() == 2, "{opt}");
    let _ = fs::remove_file(src);
}

#[test]
fn dump_regalloc_annotates_values_with_locations() {
    let src = temp_source("regalloc", GCD);
    let (ok, text, _) = run_ccc(&["--dump-regalloc", src.to_str().unwrap()]);
    assert!(ok, "{text}");
    assert!(text.contains("; rbx  live ["), "{text}");
    assert!(text.lines().any(|l| l.contains("= srem") && l.contains("; r1") && l.contains("live [")), "{text}");
    let _ = fs::remove_file(src);
}

#[test]
fn dump_ast_prints_the_parsed_program() {
    let src = temp_source("ast", GCD);
    let (ok, text, _) = run_ccc(&["--dump-ast", src.to_str().unwrap()]);
    assert!(ok && text.contains("Function {") && text.contains("name: \"gcd\"") && text.contains("While {"), "{text}");
    let _ = fs::remove_file(src);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn dump_without_output_writes_no_binary_and_with_output_does_both() {
    let src = temp_source("both", GCD);
    let out = src.with_extension("out");
    let _ = fs::remove_file(&out);
    let (ok, _, _) = run_ccc(&["--dump-ir", src.to_str().unwrap()]);
    assert!(ok && !out.exists());
    let (ok, text, _) = run_ccc(&["--dump-ir", "-o", out.to_str().unwrap(), src.to_str().unwrap()]);
    assert!(ok && text.contains("function gcd") && out.exists());
    assert_eq!(Command::new(&out).status().unwrap().code(), Some(12));
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
}

#[test]
fn command_line_errors_are_reported_with_usage() {
    let src = temp_source("cli", GCD);
    let path = src.to_str().unwrap();
    for (args, expected) in [
        (vec![path], "nothing to do"),
        (vec!["--dump-ir=bogus", path], "unknown IR stage"),
        (vec!["--dump-regalloc", "--dump-ir=ssa", path], "only applies to the optimized IR"),
        (vec!["--frobnicate", path], "unknown option"),
        (vec!["-o", "x"], "no input file"),
    ] {
        let (ok, _, err) = run_ccc(&args);
        assert!(!ok && err.contains(expected) && err.contains("usage: ccc"), "{args:?}: {err}");
    }
    let (ok, out, _) = run_ccc(&["--version"]);
    assert!(ok && out.starts_with(&format!("ccc {} (x86_64-linux-gnu)", env!("CARGO_PKG_VERSION"))), "{out}");
    let _ = fs::remove_file(src);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn compiler_output_is_reproducible() {
    // Separate processes get different hash seeds; the output must not notice.
    // `gcd` is a program whose generated code varied from run to run before phi
    // creation order was made deterministic.
    let source = GCD;
    let src = temp_source("repro", source);
    let out = src.with_extension("out");
    let mut first: Option<Vec<u8>> = None;
    for round in 0..16 {
        let (ok, _, err) = run_ccc(&["-o", out.to_str().unwrap(), src.to_str().unwrap()]);
        assert!(ok, "{err}");
        let bytes = fs::read(&out).unwrap();
        match &first {
            None => first = Some(bytes),
            Some(expected) => assert!(&bytes == expected, "output of run {round} differs from run 0"),
        }
    }
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn target_option_selects_the_backend_or_explains_what_is_supported() {
    let src = temp_source("target", GCD);
    let path = src.to_str().unwrap();
    let out = src.with_extension("out");
    for spelling in ["--target=x86_64-linux", "--target=x86_64-linux-gnu", "--target=x86_64-unknown-linux-gnu"] {
        let (ok, _, err) = run_ccc(&[spelling, "-o", out.to_str().unwrap(), path]);
        assert!(ok, "{spelling}: {err}");
        assert!(err.contains("x86-64 ELF executable"), "{err}");
        assert_eq!(Command::new(&out).status().unwrap().code(), Some(12));
    }
    for (target, expected) in [("aarch64-linux", "unsupported architecture 'aarch64'"), ("x86_64-windows", "unsupported operating system 'windows'")] {
        let (ok, _, err) = run_ccc(&[&format!("--target={target}"), "-o", out.to_str().unwrap(), path]);
        assert!(!ok && err.contains(expected) && err.contains("supported targets: x86_64-linux"), "{target}: {err}");
    }
    let _ = fs::remove_file(src);
    let _ = fs::remove_file(out);
}
