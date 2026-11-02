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
    let intervals = live_intervals(program);
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

/// Where each block starts in the linear numbering (two positions per instruction), and the
/// intervals of the values from their definitions, uses and phi operands alone.
struct Basic { intervals: Vec<Option<(usize, usize)>>, positions: Vec<usize>, ends: Vec<usize>, phi_edges: Vec<(usize, ValueId)> }

fn basic_intervals(program: &IrFunction) -> Basic {
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
    let mut ends = vec![0usize; program.blocks.len()];
    for block in &program.blocks { ends[block.id] = positions[block.id] + block.instructions.len() * 2 + 1; }
    // Phi operands are uses on incoming edges, not at the phi's block entry.
    let mut phi_edges = Vec::new();
    for block in &program.blocks {
        for inst in &block.instructions {
            if let Instruction::Phi { incoming, .. } = inst {
                for (pred, value) in incoming {
                    let edge_pos = positions.get(*pred).copied().unwrap_or(0)
                        + program.blocks.get(*pred).map(|b| b.instructions.len() * 2 + 1).unwrap_or(0);
                    touch(&mut intervals, *value, edge_pos);
                    if *pred < program.blocks.len() { phi_edges.push((*pred, *value)); }
                }
            }
        }
    }
    Basic { intervals, positions, ends, phi_edges }
}

/// The live interval of every value: from its first to its last definition, use or block in
/// which it is live, in the linear order of the blocks.
fn live_intervals(program: &IrFunction) -> Vec<Option<(usize, usize)>> {
    let Basic { mut intervals, positions, ends, phi_edges } = basic_intervals(program);
    extend_by_liveness(program, &positions, &ends, &phi_edges, &mut intervals);
    intervals
}

fn successors(term: Option<Terminator>) -> Vec<usize> {
    match term {
        Some(Terminator::Jump(t)) => vec![t],
        Some(Terminator::Branch { if_zero, if_nonzero, .. }) => vec![if_zero, if_nonzero],
        _ => Vec::new(),
    }
}

/// A value that is live across a loop back-edge must stay allocated for the whole loop, so
/// stretch each interval over the first block it is live into and the last one it is live out of.
///
/// This is backward liveness, done one value at a time instead of as a fixed point over
/// per-block *sets* of live values: a value is live into a block that uses it before defining
/// it, live out of a block that passes it to a phi on the way out, live out of a block with a
/// successor it is live into, and live into a block it is live out of unless the block defines
/// it. Walking that backwards from the uses touches every block where the value is live but
/// keeps only the two ends, so memory is linear. (Sets are quadratic when many values live
/// across many blocks: 4000 such values over 12000 blocks took 1.7 GB.)
fn extend_by_liveness(program: &IrFunction, positions: &[usize], ends: &[usize], phi_edges: &[(usize, ValueId)],
                      intervals: &mut [Option<(usize, usize)>]) {
    let (n, values) = (program.blocks.len(), intervals.len());
    let mut def_block = vec![None::<usize>; values];
    for block in &program.blocks {
        for inst in &block.instructions {
            if let Some(slot) = definition(inst).and_then(|d| def_block.get_mut(d)) { *slot = Some(block.id); }
        }
    }
    // Blocks that use each value before defining it (phi operands count at the edge instead),
    // and the predecessors that pass each value to a phi.
    let mut exposed = vec![Vec::<usize>::new(); values];
    let mut defined_in = vec![usize::MAX; values]; // the block a value was last defined in, so far
    for block in &program.blocks {
        let b = block.id;
        let mut expose = |v: ValueId, defined_in: &[usize]| {
            if let (Some(list), Some(d)) = (exposed.get_mut(v), defined_in.get(v)) { if *d != b { list.push(b); } }
        };
        for inst in &block.instructions {
            if !matches!(inst, Instruction::Phi { .. }) { for u in uses(inst) { expose(u, &defined_in); } }
            if let Some(slot) = definition(inst).and_then(|d| defined_in.get_mut(d)) { *slot = b; }
        }
        if let Some(Terminator::Branch { condition, .. } | Terminator::Return(condition)) = block.terminator {
            expose(condition, &defined_in);
        }
    }
    let mut passed_to_phi = vec![Vec::<usize>::new(); values];
    for (pred, value) in phi_edges { if let Some(list) = passed_to_phi.get_mut(*value) { list.push(*pred); } }
    let mut preds = vec![Vec::<usize>::new(); n];
    for block in &program.blocks {
        for s in successors(block.terminator) { if s < n { preds[s].push(block.id); } }
    }

    // `seen_*[b] == v` means "already walked for value v", so nothing is cleared between values.
    let (mut seen_in, mut seen_out) = (vec![usize::MAX; n], vec![usize::MAX; n]);
    let mut work = Vec::<(bool, usize)>::new(); // (live out of the block?, block)
    for v in 0..values {
        if exposed[v].is_empty() && passed_to_phi[v].is_empty() { continue; }
        work.clear();
        work.extend(exposed[v].iter().map(|b| (false, *b)));
        work.extend(passed_to_phi[v].iter().map(|b| (true, *b)));
        let (mut first, mut last) = (usize::MAX, None::<usize>);
        while let Some((live_out, b)) = work.pop() {
            if live_out {
                if seen_out[b] == v { continue; }
                seen_out[b] = v;
                last = Some(last.map_or(ends[b], |l| l.max(ends[b])));
                if def_block[v] != Some(b) { work.push((false, b)); }
            } else {
                if seen_in[b] == v { continue; }
                seen_in[b] = v;
                first = first.min(positions[b]);
                work.extend(preds[b].iter().map(|p| (true, *p)));
            }
        }
        if first != usize::MAX { touch(intervals, v, first); }
        if let Some(last) = last { touch(intervals, v, last); }
    }
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

    // ---- liveness: the walk per value must agree with the textbook dataflow ----------------

    /// The reference: backward dataflow to a fixed point over per-block *sets* of live values.
    fn liveness_by_sets(program: &IrFunction, phi_edge_uses: &[Vec<ValueId>]) -> (Vec<std::collections::HashSet<ValueId>>, Vec<std::collections::HashSet<ValueId>>) {
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
    fn intervals_by_sets(program: &ir::IrFunction) -> Vec<Option<(usize, usize)>> {
        let Basic { mut intervals, positions, phi_edges, .. } = basic_intervals(program);
        let mut phi_edge_uses = vec![Vec::<ValueId>::new(); program.blocks.len()];
        for (pred, value) in &phi_edges { phi_edge_uses[*pred].push(*value); }
        let (live_in, live_out) = liveness_by_sets(program, &phi_edge_uses);
        for block in &program.blocks {
            let start = positions[block.id];
            let end = start + block.instructions.len() * 2 + 1;
            for v in &live_in[block.id] { touch(&mut intervals, *v, start); }
            for v in &live_out[block.id] { touch(&mut intervals, *v, end); }
        }
        intervals
    }
    fn assert_same_liveness(source: &str) {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        let mut module = ir::lower(&sema::analyze(&ast).unwrap());
        ir::promote_locals_to_ssa(&mut module);
        for raw in module.functions.iter().cloned() {
            assert_eq!(live_intervals(&raw), intervals_by_sets(&raw), "unoptimized {}: {source}", raw.name);
        }
        for optimized in ir::optimize(module).functions {
            assert_eq!(live_intervals(&optimized), intervals_by_sets(&optimized), "optimized {}: {source}", optimized.name);
        }
    }
    #[test] fn liveness_agrees_with_the_dataflow_on_loops_branches_and_dead_code() {
        for source in [
            "int main(){int s=0;int i=0;while(i<10){s=s+i;i=i+1;}return s;}",
            "int main(){int a=1;int b=2;int i=0;while(i<3){int t=a;a=b;b=t+b;i=i+1;}return a+b;}",
            "int main(){int x=3;while(x){int y=x;while(y){y=y-1;}x=x-1;}return x;}",
            "int f(int a,int b){if(a){return b;}else{return a+b;} return 7;} int main(){return f(1,2);}",
            "int main(){int a=1;if(a){return 1;}return 2;return 3;}",
            "int main(){return 1; int x=2; while(x){x=x-1;} return x;}",
            "int h(int a){return a;} int main(){return h(0);}",
            "int f(int a,int b,int c){return (a&&b)||(b&&c)||!(a||c);} int main(){return f(1,0,1);}",
            "int g(int a){return a+1;} int main(){int x=1;int y=g(x);int z=g(y);while(x<5){x=x+g(z);}return x+y+z;}",
            "int main(){int a=0;int b=1;int c=2;int d=3;int e=4;int f=5;int g=6;int h=7;int i=0;while(i<4){a=a+b;b=b+c;c=c+d;d=d+e;e=e+f;f=f+g;g=g+h;h=h+a;i=i+1;}return a+b+c+d+e+f+g+h;}",
        ] {
            assert_same_liveness(source);
        }
    }
    #[test] fn liveness_agrees_with_the_dataflow_on_random_programs() {
        // Deterministic pseudo-random programs: nested loops and branches, early returns
        // (which leave unreachable blocks behind), short-circuit operators and calls.
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move |bound: usize| { state ^= state << 13; state ^= state >> 7; state ^= state << 17; (state % bound as u64) as usize };
        fn block(next: &mut dyn FnMut(usize) -> usize, depth: usize, vars: usize) -> String {
            let mut out = String::new();
            for _ in 0..1 + next(4) {
                let v = |next: &mut dyn FnMut(usize) -> usize| format!("v{}", next(vars));
                match next(10) {
                    0 | 1 if depth > 0 => out += &format!("while ({} < {}) {{ {} = {} + 1; {} }} ", v(next), 1 + next(5), "v0", "v0", block(next, depth - 1, vars)),
                    2 | 3 if depth > 0 => out += &format!("if ({}) {{ {} }} else {{ {} }} ", v(next), block(next, depth - 1, vars), block(next, depth - 1, vars)),
                    4 if depth > 0 => out += &format!("if ({} && {} || {}) {{ {} }} ", v(next), v(next), v(next), block(next, depth - 1, vars)),
                    5 if next(6) == 0 => out += &format!("return {}; ", v(next)),
                    6 => out += &format!("{} = g({}, {}); ", v(next), v(next), v(next)),
                    _ => out += &format!("{} = {} {} {}; ", v(next), v(next), ["+", "-", "*", "%", "<", "=="][next(6)], if next(3) == 0 { 3.to_string() } else { v(next) }),
                }
            }
            out
        }
        for _ in 0..400 {
            let vars = 2 + next(6);
            let mut source = String::from("int g(int a, int b){ return a + b; } int main(void){ ");
            for i in 0..vars { source += &format!("int v{i} = {}; ", i + 1); }
            source += &block(&mut next, 3, vars);
            source += &format!("return v0 + v{}; }}", vars - 1);
            let ast = Parser::new(lex(&source).unwrap()).parse().unwrap();
            let Ok(analyzed) = sema::analyze(&ast) else { continue };
            let mut module = ir::lower(&analyzed);
            ir::promote_locals_to_ssa(&mut module);
            let raw = module.functions.remove(1);
            assert_eq!(live_intervals(&raw), intervals_by_sets(&raw), "{source}");
            let optimized = ir::optimize(ir::IrModule { functions: vec![raw] }).functions.remove(0);
            assert_eq!(live_intervals(&optimized), intervals_by_sets(&optimized), "optimized: {source}");
        }
    }
    #[test] fn many_long_lived_values_over_many_blocks_need_linear_space() {
        // (a long sum is a tall expression tree: debug builds need more than a test thread's stack)
        std::thread::Builder::new().stack_size(64 << 20).spawn(many_long_lived_values).unwrap().join().unwrap();
    }
    fn many_long_lived_values() {
        // 1200 values all live to the end, across 1200 branches (about 3600 blocks). (The
        // end-to-end test in tests/e2e.rs runs a bigger one under a memory limit.)
        let n = 1200;
        let mut source = String::from("int main(void){ int y = 0; ");
        for i in 0..n { source += &format!("int a{i} = {i} + y; "); }
        for _ in 0..n { source += "if (y < 1) { y = y + 1; } "; }
        source += &format!("return ({}) % 251; }}", (0..n).map(|i| format!("a{i}")).collect::<Vec<_>>().join("+"));
        let f = function(&source);
        let intervals = live_intervals(&f);
        // Where the last block (the one that sums everything) starts in the linear numbering.
        let final_start: usize = f.blocks[..f.blocks.len() - 1].iter().map(|b| b.instructions.len() * 2 + 2).sum();
        // Every a_i is defined before the first branch and used only in that last block, so its
        // interval must reach it: (a value defined early whose interval ends before the last
        // block would be handed a register that a later value then overwrites).
        let first_branch = f.blocks[0].instructions.len() * 2 + 2;
        let spanning = intervals.iter().flatten().filter(|(s, e)| *s < first_branch && *e >= final_start).count();
        assert!(spanning >= n, "only {spanning} of {n} values stay live up to the last block");
    }
}
