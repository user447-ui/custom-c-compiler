use crate::parser::{BinaryOp,Expr,UnaryOp};
pub fn generate(expr:&Expr)->Result<Vec<u8>,String>{let mut code=Vec::new();emit_expr(expr,&mut code)?;code.extend_from_slice(&[0x48,0x89,0xc7,0xb8,60,0,0,0,0x0f,0x05]);Ok(code)}
fn emit_expr(e:&Expr,out:&mut Vec<u8>)->Result<(),String>{match e{
 Expr::Number(n)=>{out.extend_from_slice(&[0x48,0xb8]);out.extend_from_slice(&n.to_le_bytes());},
 Expr::Unary{op,expr}=>{emit_expr(expr,out)?;match op{UnaryOp::Plus=>{},UnaryOp::Minus=>out.extend_from_slice(&[0x48,0xf7,0xd8]),UnaryOp::BitNot=>out.extend_from_slice(&[0x48,0xf7,0xd0])}},
 Expr::Binary{op,left,right}=>{emit_expr(left,out)?;out.push(0x50);emit_expr(right,out)?;out.push(0x59);match op{BinaryOp::Add=>out.extend_from_slice(&[0x48,0x01,0xc8]),BinaryOp::Sub=>out.extend_from_slice(&[0x48,0x89,0xc2,0x48,0x89,0xc8,0x48,0x29,0xd0]),BinaryOp::Mul=>out.extend_from_slice(&[0x48,0x0f,0xaf,0xc1]),BinaryOp::BitAnd=>out.extend_from_slice(&[0x48,0x21,0xc8]),BinaryOp::BitOr=>out.extend_from_slice(&[0x48,0x09,0xc8]),BinaryOp::BitXor=>out.extend_from_slice(&[0x48,0x31,0xc8]),BinaryOp::Div|BinaryOp::Rem=>return Err("division and remainder code generation are not implemented yet".into())}},}Ok(())}
#[cfg(test)]mod tests{use super::*;#[test]fn emits_mov_imm64(){let b=generate(&Expr::Number(42)).unwrap();assert_eq!(&b[..2],&[0x48,0xb8]);}}
