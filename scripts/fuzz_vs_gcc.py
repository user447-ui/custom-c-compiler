#!/usr/bin/env python3
"""Differential fuzzer: random programs from the supported subset, ccc vs gcc.
Usage: scripts/fuzz_vs_gcc.py target/release/ccc 500 [first_seed]
Failing programs are kept as /tmp/fz/p<seed>.c"""
import os; os.makedirs("/tmp/fz", exist_ok=True)
import random, subprocess, sys, os
CCC = sys.argv[1]; N = int(sys.argv[2]); seed0 = int(sys.argv[3]) if len(sys.argv)>3 else 0
def gen(r):
    vars_stack=[[]]; counter=[0]
    def allvars(): return [v for s in vars_stack for v in s]
    def expr(d):
        vs=allvars()
        if d<=0 or r.random()<0.25:
            return r.choice(vs) if vs and r.random()<0.7 else str(r.randint(0,20))
        k=r.random()
        if k<0.12: return f"({r.choice(['-','!','~','+'])}{expr(d-1)})"
        if k<0.20: return f"(({expr(d-1)}) / (({expr(d-1)}) % 5 + 7))"
        if k<0.26: return f"(({expr(d-1)}) % (({expr(d-1)}) % 5 + 7))"
        op=r.choice(['+','-','*','&','|','^','==','!=','<','<=','>','>=','&&','||'])
        return f"({expr(d-1)} {op} {expr(d-1)})"
    def bounded(e): return f"(({e}) % 97)"
    def stmt(d, depth_loop):
        vs=allvars(); k=r.random()
        if k<0.30 and vs: return f"{r.choice(vs)} = {bounded(expr(3))};"
        if k<0.45:
            counter[0]+=1; n=f"v{counter[0]}"; e=bounded(expr(3)); vars_stack[-1].append(n)
            return f"int {n} = {e};"
        if k<0.55 and d>0:
            vars_stack.append([]); b=block(d-1,depth_loop); vars_stack.pop(); return b
        if k<0.70 and d>0:
            c=expr(2); t=scoped(d-1,depth_loop)
            if r.random()<0.5: return f"if ({c}) {t} else {scoped(d-1,depth_loop)}"
            return f"if ({c}) {t}"
        if k<0.85 and d>0 and depth_loop<2:
            counter[0]+=1; i=f"i{counter[0]}"; n=r.randint(1,4)
            vars_stack.append([]); body=block(d-1,depth_loop+1); vars_stack.pop()
            vars_stack[-1].append(i)  # make visible after decl
            vars_stack[-1].remove(i)
            return f"{{ int {i} = 0; while ({i} < {n}) {{ {i} = {i} + 1; {body} }} }}"
        if k<0.92 and d>0: return f"if ({expr(2)}) return {bounded(expr(2))};"
        return f"{r.choice(vs)} = {bounded(expr(2))};" if vs else "int zz0 = 1;"
    def scoped(d,dl):
        vars_stack.append([]); s=stmt(d,dl); vars_stack.pop()
        return s if s.startswith("{") else "{ "+s+" }"
    def block(d,dl):
        out=[]
        for _ in range(r.randint(1,4)): out.append(stmt(d,dl))
        return "{ "+" ".join(out)+" }"
    head=[]
    for j in range(r.randint(2,5)):
        counter[0]+=1; n=f"v{counter[0]}"; head.append(f"int {n} = {r.randint(0,20)};"); vars_stack[0].append(n)
    body=[stmt(3,0) for _ in range(r.randint(3,8))]
    vs=allvars()
    ret=" + ".join(vs[:6]) if vs else "0"
    return "int main(void) {\n  "+"\n  ".join(head+body)+f"\n  return ({ret}) % 251;\n}}\n"
bad=0; rej=0
for s in range(seed0, seed0+N):
    src=gen(random.Random(s)); p=f"/tmp/fz/p{s}"
    open(p+".c","w").write(src)
    g=subprocess.run(["gcc","-w","-O0","-o",p+"_g",p+".c"],capture_output=True)
    if g.returncode: os.remove(p+".c"); continue
    gr=subprocess.run([p+"_g"],capture_output=True,timeout=5).returncode
    c=subprocess.run([CCC,"-o",p+"_c",p+".c"],capture_output=True)
    if c.returncode:
        rej+=1; print(f"seed {s}: ccc rejected: {c.stderr.decode().strip()[:100]}"); 
        os.path.exists(p+"_g") and os.remove(p+"_g"); continue
    try: cr=subprocess.run([p+"_c"],capture_output=True,timeout=3).returncode
    except subprocess.TimeoutExpired: cr="TIMEOUT"
    if gr!=cr: bad+=1; print(f"seed {s}: MISMATCH gcc={gr} ccc={cr}")
    else:
        for ext in (".c","_g","_c"): os.remove(p+ext)
        continue
    for ext in ("_g","_c"): os.remove(p+ext)
print(f"done: {N} programs, mismatches={bad}, rejected={rej}")
