#!/usr/bin/env python3
"""Differential fuzzer: random programs from the supported subset, ccc vs gcc.

Usage: scripts/fuzz_vs_gcc.py target/release/ccc 500 [first_seed]

Each program has 0-3 helper functions (0-8 int parameters, so both register and
stack argument passing are exercised) that may call earlier helpers, plus a
`main` using loops, nested blocks, shadowing and short-circuit operators.
Values are kept small so 32-bit gcc `int` and ccc's 64-bit `int` agree and the
programs have no undefined behaviour. Failing programs are kept in /tmp/fz/.
"""
import os, random, subprocess, sys
os.makedirs("/tmp/fz", exist_ok=True)
CCC = sys.argv[1]; N = int(sys.argv[2]); seed0 = int(sys.argv[3]) if len(sys.argv) > 3 else 0

def gen_body(r, counter, params, callables, nstmts, depth):
    """Statements of one function body; returns (code, names visible at top level)."""
    vars_stack = [list(params)]
    def allvars(): return [v for s in vars_stack for v in s]
    def fresh(prefix="v"):
        counter[0] += 1; return f"{prefix}{counter[0]}"
    def bounded(e): return f"(({e}) % 97)"
    def expr(d):
        vs = allvars()
        if d <= 0 or r.random() < 0.25:
            return r.choice(vs) if vs and r.random() < 0.7 else str(r.randint(0, 20))
        k = r.random()
        if k < 0.10: return f"({r.choice(['-','!','~','+'])}{expr(d-1)})"
        if k < 0.17: return f"(({expr(d-1)}) / (({expr(d-1)}) % 5 + 7))"
        if k < 0.22: return f"(({expr(d-1)}) % (({expr(d-1)}) % 5 + 7))"
        if k < 0.34 and callables:
            name, n = r.choice(callables)
            return f"{name}({', '.join(bounded(expr(d-1)) for _ in range(n))})"
        op = r.choice(['+','-','*','&','|','^','==','!=','<','<=','>','>=','&&','||'])
        return f"({expr(d-1)} {op} {expr(d-1)})"
    def stmt(d, loops):
        vs = allvars(); k = r.random()
        if k < 0.28 and vs: return f"{r.choice(vs)} = {bounded(expr(3))};"
        if k < 0.42:
            e = bounded(expr(3)); n = fresh(); vars_stack[-1].append(n); return f"int {n} = {e};"
        if k < 0.50 and d > 0:
            vars_stack.append([]); b = block(d-1, loops); vars_stack.pop(); return b
        if k < 0.64 and d > 0:
            c = expr(2); t = scoped(d-1, loops)
            return f"if ({c}) {t} else {scoped(d-1, loops)}" if r.random() < 0.5 else f"if ({c}) {t}"
        if k < 0.78 and d > 0 and loops < 2:
            i = fresh("i"); n = r.randint(1, 4)
            vars_stack.append([]); body = block(d-1, loops+1); vars_stack.pop()
            return f"{{ int {i} = 0; while ({i} < {n}) {{ {i} = {i} + 1; {body} }} }}"
        if k < 0.86 and d > 0: return f"if ({expr(2)}) return {bounded(expr(2))};"
        if k < 0.92 and callables:
            name, n = r.choice(callables)
            return f"{name}({', '.join(bounded(expr(2)) for _ in range(n))});"
        return f"{r.choice(vs)} = {bounded(expr(2))};" if vs else ";"
    def scoped(d, loops):
        vars_stack.append([]); s = stmt(d, loops); vars_stack.pop()
        return s if s.startswith("{") else "{ " + s + " }"
    def block(d, loops): return "{ " + " ".join(stmt(d, loops) for _ in range(r.randint(1, 4))) + " }"
    head = []
    for _ in range(r.randint(1, 4)):
        n = fresh(); head.append(f"int {n} = {r.randint(0, 20)};"); vars_stack[0].append(n)
    body = [stmt(depth, 0) for _ in range(nstmts)]
    vs = allvars()
    ret = " + ".join(r.sample(vs, min(len(vs), 6))) if vs else "0"
    return "\n  ".join(head + body) + f"\n  return ({ret}) % 251;"

def gen(r):
    counter = [0]; funcs = []; callables = []
    for k in range(r.randint(0, 3)):
        name = f"f{k+1}"; n = r.choice([0, 1, 2, 3, 5, 6, 7, 8])
        params = []
        for _ in range(n):
            counter[0] += 1; params.append(f"p{counter[0]}")
        body = gen_body(r, counter, params, list(callables), r.randint(2, 5), 2)
        plist = ", ".join(f"int {p}" for p in params) or "void"
        funcs.append(f"int {name}({plist}) {{\n  {body}\n}}\n")
        callables.append((name, n))
    main = gen_body(r, counter, [], callables, r.randint(3, 8), 3)
    return "\n".join(funcs) + f"\nint main(void) {{\n  {main}\n}}\n"

bad = rej = 0
for s in range(seed0, seed0 + N):
    src = gen(random.Random(s)); p = f"/tmp/fz/p{s}"
    open(p + ".c", "w").write(src)
    g = subprocess.run(["gcc", "-w", "-O0", "-o", p + "_g", p + ".c"], capture_output=True)
    if g.returncode: os.remove(p + ".c"); continue
    try: gr = subprocess.run([p + "_g"], capture_output=True, timeout=5).returncode
    except subprocess.TimeoutExpired: os.remove(p + ".c"); os.remove(p + "_g"); continue
    c = subprocess.run([CCC, "-o", p + "_c", p + ".c"], capture_output=True)
    if c.returncode:
        rej += 1; print(f"seed {s}: ccc rejected: {c.stderr.decode().strip()[:100]}")
        os.path.exists(p + "_g") and os.remove(p + "_g"); continue
    try: cr = subprocess.run([p + "_c"], capture_output=True, timeout=5).returncode
    except subprocess.TimeoutExpired: cr = "TIMEOUT"
    if gr != cr:
        bad += 1; print(f"seed {s}: MISMATCH gcc={gr} ccc={cr}")
        for ext in ("_g", "_c"): os.remove(p + ext)
    else:
        for ext in (".c", "_g", "_c"): os.remove(p + ext)
print(f"done: {N} programs, mismatches={bad}, rejected={rej}")
