use crate::ir::{Instruction, IrProgram, Terminator, ValueId};

/// Physical registers used for long-lived virtual values. The generated program
/// is a standalone Linux process, so no caller-preserved ABI state is required.
pub const ALLOCATABLE_REGISTERS: [u8; 5] = [3, 12, 13, 14, 15]; // RBX, R12-R15

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location { Register(u8), Spill(usize) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    pub locations: Vec<Location>,
    pub spill_count: usize,
    pub intervals: Vec<Option<(usize, usize)>>,
}

#[derive(Debug, Clone, Copy)]
struct Interval { value: ValueId, start: usize, end: usize }

pub fn allocate(program: &IrProgram) -> Allocation {
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
    for block in &program.blocks {
        for inst in &block.instructions {
            if let Instruction::Phi { incoming, .. } = inst {
                for (pred, value) in incoming {
                    let edge_pos = positions.get(*pred).copied().unwrap_or(0)
                        + program.blocks.get(*pred).map(|b| b.instructions.len() * 2 + 1).unwrap_or(0);
                    touch(&mut intervals, *value, edge_pos);
                }
            }
        }
    }
    let mut ordered: Vec<Interval> = intervals.iter().enumerate().filter_map(|(value, range)|
        range.map(|(start, end)| Interval { value, start, end })).collect();
    ordered.sort_by_key(|i| (i.start, i.end, i.value));
    let mut locations = vec![Location::Spill(0); program.value_count];
    let mut active = Vec::<Interval>::new();
    let mut spill_count = 0usize;
    for current in ordered {
        active.retain(|i| i.end >= current.start);
        let occupied: Vec<u8> = active.iter().filter_map(|i| match locations[i.value] {
            Location::Register(r) => Some(r), Location::Spill(_) => None,
        }).collect();
        if let Some(reg) = ALLOCATABLE_REGISTERS.iter().copied().find(|r| !occupied.contains(r)) {
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
        Instruction::Phi { .. } | Instruction::Const { .. } | Instruction::LoadLocal { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ir, lexer::lex, parser::Parser, sema};
    #[test]
    fn assigns_registers_and_tracks_live_ranges() {
        let ast = Parser::new(lex("int main(){int x=1;int y=2;int z=3;return x+y+z;}").unwrap()).parse().unwrap();
        let mut p = ir::lower(&sema::analyze(&ast).unwrap());
        ir::promote_locals_to_ssa(&mut p);
        let a = allocate(&p);
        assert!(a.locations.iter().any(|l| matches!(l, Location::Register(_))));
        assert!(a.intervals.iter().any(Option::is_some));
        assert_eq!(a.locations.len(), p.value_count);
    }
}
