use std::{env, fs, process};

const BASE: u64 = 0x400000;
#[derive(Debug, Clone, PartialEq)]
enum Tok { Id(String), Num(i64), Lp, Rp, Lb, Rb, Semi, End, Other(char) }
fn lex(s: &str) -> Result<Vec<Tok>, String> {
 let c: Vec<char> = s.chars().collect(); let mut i=0; let mut v=Vec::new();
 while i<c.len() { let x=c[i]; if x.is_whitespace(){i+=1;continue}
 if x=='/' && i+1<c.len() && c[i+1]=='/' {i+=2;while i<c.len()&&c[i]!='\n'{i+=1}continue}
 if x.is_ascii_alphabetic()||x=='_' {let a=i;i+=1;while i<c.len()&&(c[i].is_ascii_alphanumeric()||c[i]=='_'){i+=1}v.push(Tok::Id(c[a..i].iter().collect()));continue}
 if x.is_ascii_digit(){let a=i;i+=1;while i<c.len()&&c[i].is_ascii_digit(){i+=1}let n:String=c[a..i].iter().collect();v.push(Tok::Num(n.parse().map_err(|_|format!("integer out of range: {n}"))?));continue}
 v.push(match x{'('=>Tok::Lp,')'=>Tok::Rp,'{'=>Tok::Lb,'}'=>Tok::Rb,';'=>Tok::Semi,z=>Tok::Other(z)});i+=1;
 } v.push(Tok::End);Ok(v)
}
struct Parser{v:Vec<Tok>,p:usize}
impl Parser{
 fn take(&mut self)->Tok{let t=self.v[self.p].clone();self.p+=1;t}
 fn expect(&mut self,w:Tok)->Result<(),String>{let t=self.take();if t==w{Ok(())}else{Err(format!("expected {w:?}, got {t:?}"))}}
 fn id(&mut self,w:&str)->Result<(),String>{match self.take(){Tok::Id(s)if s==w=>Ok(()),t=>Err(format!("expected {w}, got {t:?}"))}}
 fn parse(mut self)->Result<u8,String>{self.id("int")?;self.id("main")?;self.expect(Tok::Lp)?;if self.v[self.p]==Tok::Id("void".into()){self.take();}self.expect(Tok::Rp)?;self.expect(Tok::Lb)?;self.id("return")?;let n=match self.take(){Tok::Num(n)if(0..=255).contains(&n)=>n,Tok::Num(n)=>return Err(format!("return value {n} must be in 0..255")),t=>return Err(format!("expected integer literal, got {t:?}"))};self.expect(Tok::Semi)?;self.expect(Tok::Rb)?;if self.v[self.p]!=Tok::End{return Err(format!("unexpected token {:?}",self.v[self.p]))}Ok(n as u8)}
}
fn p16(b:&mut[u8],o:usize,n:u16){b[o..o+2].copy_from_slice(&n.to_le_bytes())}fn p32(b:&mut[u8],o:usize,n:u32){b[o..o+4].copy_from_slice(&n.to_le_bytes())}fn p64(b:&mut[u8],o:usize,n:u64){b[o..o+8].copy_from_slice(&n.to_le_bytes())}
fn elf(code:u8)->Vec<u8>{let machine=[0xb8,60,0,0,0,0xbf,code,0,0,0,0x0f,0x05];let off=120usize;let size=off+machine.len();let mut b=vec![0u8;size];b[..4].copy_from_slice(b"\x7fELF");b[4]=2;b[5]=1;b[6]=1;p16(&mut b,16,2);p16(&mut b,18,62);p32(&mut b,20,1);p64(&mut b,24,BASE+off as u64);p64(&mut b,32,64);p16(&mut b,52,64);p16(&mut b,54,56);p16(&mut b,56,1);p32(&mut b,64,1);p32(&mut b,68,5);p64(&mut b,72,0);p64(&mut b,80,BASE);p64(&mut b,88,BASE);p64(&mut b,96,size as u64);p64(&mut b,104,size as u64);p64(&mut b,112,0x1000);b[off..].copy_from_slice(&machine);b}
fn run()->Result<(),String>{let a:Vec<String>=env::args().collect();if a.len()==2&&a[1]=="--version"{println!("ccc 0.1.0 (x86_64-linux-gnu)");return Ok(())}if a.len()!=4||a[1]!="-o"{return Err("usage: ccc -o OUTPUT INPUT.c".into())}let s=fs::read_to_string(&a[3]).map_err(|e|format!("read {}: {e}",a[3]))?;let n=Parser{v:lex(&s)?,p:0}.parse()?;fs::write(&a[2],elf(n)).map_err(|e|format!("write {}: {e}",a[2]))?;#[cfg(unix)]{use std::os::unix::fs::PermissionsExt;fs::set_permissions(&a[2],fs::Permissions::from_mode(0o755)).map_err(|e|e.to_string())?}eprintln!("ccc: emitted native ELF: {}",a[2]);Ok(())}
fn main(){if let Err(e)=run(){eprintln!("ccc: error: {e}");process::exit(1)}}
#[cfg(test)]mod tests{use super::*;#[test]fn parses_return(){assert_eq!(Parser{v:lex("int main(void){return 42;}").unwrap(),p:0}.parse().unwrap(),42)}#[test]fn rejects_unknown_expr(){assert!(Parser{v:lex("int main(){return x;}").unwrap(),p:0}.parse().is_err())}#[test]fn elf_magic(){assert_eq!(&elf(0)[..4],b"\x7fELF")}}
