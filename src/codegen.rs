use std::collections::HashMap;
use crate::ast::{BinaryOp, Expr, Program, Stmt, UnaryOp};

pub fn generate(p: &Program) -> Result<Vec<u8>, String> {
    let mut names=Vec::new(); collect(&p.statements,&mut names)?;
    let slots:HashMap<String,i32>=names.iter().enumerate().map(|(i,n)|(n.clone(),((i+1)*8) as i32)).collect();
    let frame=((names.len()*8+15)/16*16) as u32;
    let mut g=Gen{c:Vec::new(),slots,labels:Vec::new(),patches:Vec::new()};
    g.c.extend_from_slice(&[0x55,0x48,0x89,0xe5]);
    if frame>0 { g.c.extend_from_slice(&[0x48,0x81,0xec]); g.c.extend_from_slice(&frame.to_le_bytes()); }
    for s in &p.statements { g.stmt(s)?; }
    g.c.extend_from_slice(&[0x48,0xb8]); g.c.extend_from_slice(&0i64.to_le_bytes());
    g.c.extend_from_slice(&[0x48,0x89,0xc7,0xb8,60,0,0,0,0x0f,0x05]);
    for (at,l) in g.patches.clone() {
        let target=*g.labels.get(l).ok_or("unresolved branch label")? as i64;
        let delta=i32::try_from(target-(at+4) as i64).map_err(|_|"branch out of range")?;
        g.c[at..at+4].copy_from_slice(&delta.to_le_bytes());
    }
    Ok(g.c)
}
fn collect(ss:&[Stmt],v:&mut Vec<String>)->Result<(),String>{
    for s in ss { match s {
        Stmt::Declaration{name,..}=>{if v.contains(name){return Err(format!("duplicate local variable '{name}' (shadowing not supported)"));}v.push(name.clone());},
        Stmt::Block(b)=>collect(b,v)?,
        Stmt::If{then_branch,else_branch,..}=>{collect_one(then_branch,v)?;if let Some(x)=else_branch{collect_one(x,v)?}},
        Stmt::While{body,..}=>collect_one(body,v)?,
        _=>{},
    }} Ok(())
}
fn collect_one(s:&Stmt,v:&mut Vec<String>)->Result<(),String>{match s{
    Stmt::Block(b)=>collect(b,v),
    Stmt::Declaration{name,..}=>{if v.contains(name){return Err(format!("duplicate local variable '{name}' (shadowing not supported)"));}v.push(name.clone());Ok(())},
    Stmt::If{then_branch,else_branch,..}=>{collect_one(then_branch,v)?;if let Some(x)=else_branch{collect_one(x,v)?}Ok(())},
    Stmt::While{body,..}=>collect_one(body,v),_=>Ok(())
}}
struct Gen{c:Vec<u8>,slots:HashMap<String,i32>,labels:Vec<usize>,patches:Vec<(usize,usize)>}
impl Gen {
    fn label(&mut self)->usize{let i=self.labels.len();self.labels.push(usize::MAX);i}
    fn mark(&mut self,l:usize){self.labels[l]=self.c.len();}
    fn jump(&mut self,cond:bool,l:usize){if cond{self.c.extend_from_slice(&[0x0f,0x84]);}else{self.c.push(0xe9);}let at=self.c.len();self.c.extend_from_slice(&[0;4]);self.patches.push((at,l));}
    fn slot(&self,n:&str)->Result<i32,String>{self.slots.get(n).copied().ok_or_else(||format!("use of undeclared variable '{n}'"))}
    fn stmt(&mut self,s:&Stmt)->Result<(),String>{match s{
        Stmt::Declaration{name,initializer}=>if let Some(e)=initializer{self.expr(e)?;let d=self.slot(name)?;self.c.extend_from_slice(&[0x48,0x89,0x85]);self.c.extend_from_slice(&(-d).to_le_bytes());},
        Stmt::Assignment{name,value}=>{self.expr(value)?;let d=self.slot(name)?;self.c.extend_from_slice(&[0x48,0x89,0x85]);self.c.extend_from_slice(&(-d).to_le_bytes());},
        Stmt::Return(e)=>{self.expr(e)?;self.c.extend_from_slice(&[0x48,0x89,0xc7,0xb8,60,0,0,0,0x0f,0x05]);},
        Stmt::Block(b)=>for x in b{self.stmt(x)?},
        Stmt::If{condition,then_branch,else_branch}=>{let other=self.label();let end=self.label();self.expr(condition)?;self.c.extend_from_slice(&[0x48,0x85,0xc0]);self.jump(true,other);self.stmt(then_branch)?;if let Some(x)=else_branch{self.jump(false,end);self.mark(other);self.stmt(x)?;self.mark(end);}else{self.mark(other);}},
        Stmt::While{condition,body}=>{let start=self.label();let end=self.label();self.mark(start);self.expr(condition)?;self.c.extend_from_slice(&[0x48,0x85,0xc0]);self.jump(true,end);self.stmt(body)?;self.jump(false,start);self.mark(end);},
    } Ok(())}
    fn expr(&mut self,e:&Expr)->Result<(),String>{match e{
        Expr::Number(n)=>{self.c.extend_from_slice(&[0x48,0xb8]);self.c.extend_from_slice(&n.to_le_bytes());},
        Expr::Variable(n)=>{let d=self.slot(n)?;self.c.extend_from_slice(&[0x48,0x8b,0x85]);self.c.extend_from_slice(&(-d).to_le_bytes());},
        Expr::Unary{op,expr}=>{self.expr(expr)?;match op{UnaryOp::Plus=>{},UnaryOp::Minus=>self.c.extend_from_slice(&[0x48,0xf7,0xd8]),UnaryOp::BitNot=>self.c.extend_from_slice(&[0x48,0xf7,0xd0]),UnaryOp::LogicalNot=>self.c.extend_from_slice(&[0x48,0x85,0xc0,0x0f,0x94,0xc0,0x48,0x0f,0xb6,0xc0])}},
        Expr::Binary{op,left,right}=>{
            self.expr(left)?;self.c.push(0x50);self.expr(right)?;self.c.push(0x59);
            match op{
                BinaryOp::Add=>self.c.extend_from_slice(&[0x48,0x01,0xc8]),
                BinaryOp::Sub=>self.c.extend_from_slice(&[0x48,0x89,0xc2,0x48,0x89,0xc8,0x48,0x29,0xd0]),
                BinaryOp::Mul=>self.c.extend_from_slice(&[0x48,0x0f,0xaf,0xc1]),
                BinaryOp::BitAnd=>self.c.extend_from_slice(&[0x48,0x21,0xc8]),
                BinaryOp::BitOr=>self.c.extend_from_slice(&[0x48,0x09,0xc8]),
                BinaryOp::BitXor=>self.c.extend_from_slice(&[0x48,0x31,0xc8]),
                BinaryOp::Div|BinaryOp::Rem=>{self.c.extend_from_slice(&[0x48,0x89,0xc6,0x48,0x89,0xc8,0x48,0x99,0x48,0xf7,0xfe]);if *op==BinaryOp::Rem{self.c.extend_from_slice(&[0x48,0x89,0xd0]);}},
                BinaryOp::Equal|BinaryOp::NotEqual|BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual=>{
                    self.c.extend_from_slice(&[0x48,0x39,0xc1]);
                    let cc=match op{BinaryOp::Equal=>0x94,BinaryOp::NotEqual=>0x95,BinaryOp::Less=>0x9c,BinaryOp::LessEqual=>0x9e,BinaryOp::Greater=>0x9f,BinaryOp::GreaterEqual=>0x9d,_=>unreachable!()};
                    self.c.extend_from_slice(&[0x0f,cc,0xc0,0x48,0x0f,0xb6,0xc0]);
                },
                BinaryOp::LogicalAnd|BinaryOp::LogicalOr=>{
                    self.c.extend_from_slice(&[0x48,0x85,0xc9,0x0f,0x95,0xc1,0x48,0x85,0xc0,0x0f,0x95,0xc0]);
                    if *op==BinaryOp::LogicalAnd{self.c.extend_from_slice(&[0x20,0xc8]);}else{self.c.extend_from_slice(&[0x08,0xc8]);}
                    self.c.extend_from_slice(&[0x48,0x0f,0xb6,0xc0]);
                },
            }
        },
    } Ok(())}
}
