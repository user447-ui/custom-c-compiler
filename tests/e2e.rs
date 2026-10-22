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
