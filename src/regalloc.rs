//! Linear-scan register allocation over block-level liveness.
//!
//! The allocator is target independent: the backend supplies the list of
//! registers it may hand out (as opaque numbers) and interprets the result.

use crate::ir::{Instruction, IrFunction, Terminator, ValueId};

/// A physical register, numbered however the target backend numbers its registers.
pub type PhysReg = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location { Register(PhysReg), Spill(usize) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    pub locations: Vec<Location>,
    pub spill_count: usize,
    pub intervals: Vec<Option<(usize, usize)>>,
}

#[derive(Debug, Clone, Copy)]
struct Interval { value: ValueId, start: usize, end: usize }

/// Assign every value a register from `registers` or a spill slot. Values with
/// overlapping live ranges never share a register.
pub fn allocate(program: &IrFunction, registers: &[PhysReg]) -> Allocation {
    let mut intervals = vec![None::<(usize, usize)>; program.value_count];
    let mut positions = vec![0usize; program.blocks.len()];
    let mut pos = 0usize;
    for block in &program.blocks {
        if block.id < positions.len() { positions[block.id] = pos; }
        for inst in &block.instructions {
            if let Some(dst) = definition(inst) { touch(&mut intervals, dst, pos); }
            for src in uses(inst) { touch(&mut intervals, src, pos); }
            pos += 2;
        }
        if let Some(term) = block.terminator {
            match term {
                Terminator::Branch { condition, .. } | Terminator::Return(condition) => touch(&mut intervals, condition, pos),
                Terminator::Jump(_) => {}
            }
        }
        pos += 2;
    }
    // Phi operands are uses on incoming edges, not at the phi's block entry.
    let mut phi_edge_uses = vec![Vec::<ValueId>::new(); program.blocks.len()];
    for block in &program.blocks {
        for inst in &block.instructions {
            if let Instruction::Phi { incoming, .. } = inst {
                for (pred, value) in incoming {
                    let edge_pos = positions.get(*pred).copied().unwrap_or(0)
                        + program.blocks.get(*pred).map(|b| b.instructions.len() * 2 + 1).unwrap_or(0);
                    touch(&mut intervals, *value, edge_pos);
                    if let Some(v) = phi_edge_uses.get_mut(*pred) { v.push(*value); }
                }
            }
        }
    }
    // A value that is live across a loop back-edge must stay allocated for the
    // whole loop, so extend intervals using real block-level liveness.
    let (live_in, live_out) = liveness(program, &phi_edge_uses);
    for block in &program.blocks {
        let start = positions[block.id];
        let end = start + block.instructions.len() * 2 + 1;
        for v in &live_in[block.id] { touch(&mut intervals, *v, start); }
        for v in &live_out[block.id] { touch(&mut intervals, *v, end); }
    }
    let mut ordered: Vec<Interval> = intervals.iter().enumerate().filter_map(|(value, range)|
        range.map(|(start, end)| Interval { value, start, end })).collect();
    ordered.sort_by_key(|i| (i.start, i.end, i.value));
    let mut locations = vec![Location::Spill(0); program.value_count];
    let mut active = Vec::<Interval>::new();
    let mut spill_count = 0usize;
    for current in ordered {
        active.retain(|i| i.end >= current.start);
        let occupied: Vec<PhysReg> = active.iter().filter_map(|i| match locations[i.value] {
            Location::Register(r) => Some(r), Location::Spill(_) => None,
        }).collect();
        if let Some(reg) = registers.iter().copied().find(|r| !occupied.contains(r)) {
            locations[current.value] = Location::Register(reg);
            active.push(current);
            active.sort_by_key(|i| i.end);
        } else {
            let victim = active.iter().enumerate().filter(|(_, i)| matches!(locations[i.value], Location::Register(_)))
                .max_by_key(|(_, i)| i.end).map(|(index, interval)| (index, *interval));
            if let Some((index, victim_interval)) = victim.filter(|(_, i)| i.end > current.end) {
                let reg = match locations[victim_interval.value] { Location::Register(r) => r, _ => unreachable!() };
                locations[victim_interval.value] = Location::Spill(spill_count);
                spill_count += 1;
                locations[current.value] = Location::Register(reg);
                active.remove(index);
                active.push(current);
                active.sort_by_key(|i| i.end);
            } else {
                locations[current.value] = Location::Spill(spill_count);
                spill_count += 1;
            }
        }
    }
    Allocation { locations, spill_count, intervals }
}

fn successors(term: Option<Terminator>) -> Vec<usize> {
    match term {
        Some(Terminator::Jump(t)) => vec![t],
        Some(Terminator::Branch { if_zero, if_nonzero, .. }) => vec![if_zero, if_nonzero],
        _ => Vec::new(),
    }
}

/// Classic backward dataflow: live_in[b] = use[b] U (live_out[b] - def[b]),
/// live_out[b] = U live_in[succ] U (phi operands flowing out of b).
fn liveness(program: &IrFunction, phi_edge_uses: &[Vec<ValueId>]) -> (Vec<std::collections::HashSet<ValueId>>, Vec<std::collections::HashSet<ValueId>>) {
    use std::collections::HashSet;
    let n = program.blocks.len();
    let mut uses_b = vec![HashSet::<ValueId>::new(); n];
    let mut defs_b = vec![HashSet::<ValueId>::new(); n];
    for block in &program.blocks {
        let b = block.id;
        for inst in &block.instructions {
            if !matches!(inst, Instruction::Phi { .. }) {
                for u in uses(inst) { if !defs_b[b].contains(&u) { uses_b[b].insert(u); } }
            }
            if let Some(d) = definition(inst) { defs_b[b].insert(d); }
        }
        match block.terminator {
            Some(Terminator::Branch { condition, .. }) | Some(Terminator::Return(condition)) =>
                if !defs_b[b].contains(&condition) { uses_b[b].insert(condition); },
            _ => {}
        }
    }
    let mut live_in = vec![HashSet::<ValueId>::new(); n];
    let mut live_out = vec![HashSet::<ValueId>::new(); n];
    loop {
        let mut changed = false;
        for block in program.blocks.iter().rev() {
            let b = block.id;
            let mut out: HashSet<ValueId> = phi_edge_uses[b].iter().copied().collect();
            for s in successors(block.terminator) { if s < n { out.extend(live_in[s].iter().copied()); } }
            let mut inn = uses_b[b].clone();
            inn.extend(out.iter().copied().filter(|v| !defs_b[b].contains(v)));
            if out != live_out[b] { live_out[b] = out; changed = true; }
            if inn != live_in[b] { live_in[b] = inn; changed = true; }
        }
        if !changed { break; }
    }
    (live_in, live_out)
}

fn touch(intervals: &mut [Option<(usize, usize)>], value: ValueId, position: usize) {
    if let Some(range) = intervals.get_mut(value) {
        *range = Some(match *range {
            Some((start, end)) => (start.min(position), end.max(position)),
            None => (position, position),
        });
    }
}
fn definition(inst: &Instruction) -> Option<ValueId> {
    match inst {
        Instruction::Const { dst, .. } | Instruction::LoadLocal { dst, .. }
        | Instruction::Param { dst, .. } | Instruction::Call { dst, .. }
        | Instruction::Unary { dst, .. } | Instruction::Binary { dst, .. }
        | Instruction::ToBool { dst, .. } | Instruction::Phi { dst, .. } => Some(*dst),
        Instruction::StoreLocal { .. } => None,
    }
}
fn uses(inst: &Instruction) -> Vec<ValueId> {
    match inst {
        Instruction::Unary { src, .. } | Instruction::ToBool { src, .. } => vec![*src],
        Instruction::Binary { lhs, rhs, .. } => vec![*lhs, *rhs],
        Instruction::StoreLocal { src, .. } => vec![*src],
        Instruction::Call { args, .. } => args.clone(),
        Instruction::Phi { .. } | Instruction::Const { .. } | Instruction::LoadLocal { .. }
        | Instruction::Param { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ir, lexer::lex, parser::Parser, sema};

    /// Arbitrary register numbers: the allocator must not care which they are.
    const FIVE: [PhysReg; 5] = [10, 11, 12, 13, 14];

    fn function(source: &str) -> ir::IrFunction {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        let mut m = ir::lower(&sema::analyze(&ast).unwrap());
        ir::promote_locals_to_ssa(&mut m);
        m.functions.remove(0)
    }
    /// No two values whose live ranges overlap may share a register.
    fn assert_no_conflicts(a: &Allocation) {
        for (i, (li, ii)) in a.locations.iter().zip(&a.intervals).enumerate() {
            for (j, (lj, ij)) in a.locations.iter().zip(&a.intervals).enumerate().skip(i + 1) {
                let (Some((s1, e1)), Some((s2, e2))) = (ii, ij) else { continue };
                if let (Location::Register(r1), Location::Register(r2)) = (li, lj) {
                    assert!(r1 != r2 || e1 < s2 || e2 < s1, "%{i} and %{j} overlap in register {r1}");
                }
            }
        }
    }

    #[test]
    fn assigns_registers_and_tracks_live_ranges() {
        let p = function("int main(){int x=1;int y=2;int z=3;return x+y+z;}");
        let a = allocate(&p, &FIVE);
        assert!(a.locations.iter().any(|l| matches!(l, Location::Register(_))));
        assert!(a.intervals.iter().any(Option::is_some));
        assert_eq!(a.locations.len(), p.value_count);
    }
    #[test]
    fn only_uses_the_registers_it_was_given() {
        let p = function("int f(int a,int b,int c,int d){return a+b*c-d;} int main(){return 0;}");
        for set in [&[7u8][..], &[7, 9], &FIVE] {
            let a = allocate(&p, set);
            for l in &a.locations { if let Location::Register(r) = l { assert!(set.contains(r), "{r} not in {set:?}"); } }
        }
    }
    #[test]
    fn fewer_registers_means_more_spills_but_never_conflicts() {
        let p = function("int f(int a,int b,int c,int d,int e,int g){return a+b*c-d+e*g+a*g;} int main(){return 0;}");
        let (one, two, five) = (allocate(&p, &[7]), allocate(&p, &[7, 9]), allocate(&p, &FIVE));
        assert!(one.spill_count > 0 && one.spill_count >= two.spill_count && two.spill_count >= five.spill_count,
            "{} {} {}", one.spill_count, two.spill_count, five.spill_count);
        for a in [&one, &two, &five] { assert_no_conflicts(a); }
    }
    #[test]
    fn no_registers_spills_everything() {
        let p = function("int main(){int x=1;int y=2;return x+y;}");
        let a = allocate(&p, &[]);
        assert!(a.locations.iter().all(|l| matches!(l, Location::Spill(_))));
    }
}
