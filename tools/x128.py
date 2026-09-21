#!/usr/bin/env python3
"""Assembler/emulator for Generic's experimental x128 reference ISA."""
import argparse
import ast
from pathlib import Path
import sys

MASK = (1 << 128) - 1
INSN = 20
RAM = 64 * 1024
OPS = {
    "nop": 0, "movi": 1, "mov": 2, "add": 3, "sub": 4, "and": 5,
    "or": 6, "xor": 7, "cmpeq": 8, "jmp": 9, "jz": 10, "jnz": 11,
    "load8": 12, "store8": 13, "out": 14, "halt": 15,
}

class Error(Exception):
    pass

def reg(text):
    text = text.strip().lower()
    if not text.startswith("r"):
        raise Error(f"expected register, got {text!r}")
    try:
        n = int(text[1:])
    except ValueError as exc:
        raise Error(f"invalid register {text!r}") from exc
    if not 0 <= n < 16:
        raise Error(f"register out of range: r{n}")
    return n

def imm(text, labels):
    text = text.strip()
    if text in labels:
        return labels[text]
    if len(text) >= 3 and text[0] == text[-1] == "'":
        value = ast.literal_eval(text).encode()
        if len(value) != 1:
            raise Error("character literal must be one byte")
        return value[0]
    try:
        value = int(text, 0)
    except ValueError as exc:
        raise Error(f"invalid immediate or label: {text!r}") from exc
    value &= MASK
    return value

def expanded(source):
    out = []
    for line_no, raw in enumerate(source.splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if line.startswith("puts "):
            try:
                text = ast.literal_eval(line[5:].strip())
            except (ValueError, SyntaxError) as exc:
                raise Error(f"line {line_no}: invalid puts string") from exc
            if not isinstance(text, str):
                raise Error(f"line {line_no}: puts expects a string")
            for byte in text.encode():
                out += [(line_no, f"movi r1, {byte}"), (line_no, "out 0, r1")]
        else:
            out.append((line_no, line))
    return out

def assemble(source):
    lines = expanded(source)
    labels, body, pc = {}, [], 0
    for line_no, line in lines:
        if ":" in line:
            label, rest = line.split(":", 1)
            label = label.strip()
            if label and not any(c.isspace() for c in label):
                if label in labels:
                    raise Error(f"line {line_no}: duplicate label {label}")
                labels[label], line = pc, rest.strip()
        if line:
            body.append((line_no, line)); pc += INSN

    image = bytearray()
    for line_no, line in body:
        try:
            parts = line.split(None, 1)
            op = parts[0].lower()
            if op not in OPS:
                raise Error(f"unknown instruction {op}")
            a = [x.strip() for x in parts[1].split(",")] if len(parts) == 2 else []
            rd = ra = rb = value = 0
            if op == "nop":
                if a: raise Error("nop takes no operands")
            elif op == "movi":
                if len(a) != 2: raise Error("movi rD, IMM")
                rd, value = reg(a[0]), imm(a[1], labels)
            elif op == "mov":
                if len(a) != 2: raise Error("mov rD, rA")
                rd, ra = reg(a[0]), reg(a[1])
            elif op in {"add", "sub", "and", "or", "xor"}:
                if len(a) != 3: raise Error(f"{op} rD, rA, rB")
                rd, ra, rb = reg(a[0]), reg(a[1]), reg(a[2])
            elif op == "cmpeq":
                if len(a) != 2: raise Error("cmpeq rA, rB")
                ra, rb = reg(a[0]), reg(a[1])
            elif op in {"jmp", "jz", "jnz", "halt"}:
                if len(a) != 1: raise Error(f"{op} IMM")
                value = imm(a[0], labels)
            elif op == "load8":
                if len(a) != 3: raise Error("load8 rD, rA, OFFSET")
                rd, ra, value = reg(a[0]), reg(a[1]), imm(a[2], labels)
            elif op == "store8":
                if len(a) != 3: raise Error("store8 rA, rB, OFFSET")
                ra, rb, value = reg(a[0]), reg(a[1]), imm(a[2], labels)
            elif op == "out":
                if len(a) != 2: raise Error("out PORT, rA")
                value, ra = imm(a[0], labels), reg(a[1])
            image += bytes((OPS[op], rd, ra, rb)) + value.to_bytes(16, "little")
        except Error as exc:
            raise Error(f"line {line_no}: {exc}") from exc
    return bytes(image)

def run(image):
    if len(image) > RAM:
        raise Error("image exceeds reference RAM")
    mem = bytearray(RAM); mem[:len(image)] = image
    r, pc, zero, output = [0] * 16, 0, False, bytearray()
    for _ in range(100_000):
        if pc > RAM - INSN:
            raise Error(f"instruction fetch fault at 0x{pc:x}")
        op, rd, ra, rb = mem[pc:pc+4]
        if op not in OPS.values() or max(rd, ra, rb) >= 16:
            raise Error(f"invalid instruction at 0x{pc:x}")
        value = int.from_bytes(mem[pc+4:pc+INSN], "little")
        next_pc = (pc + INSN) & MASK
        if op == OPS["movi"]: r[rd] = value
        elif op == OPS["mov"]: r[rd] = r[ra]
        elif op == OPS["add"]: r[rd] = (r[ra] + r[rb]) & MASK
        elif op == OPS["sub"]: r[rd] = (r[ra] - r[rb]) & MASK
        elif op == OPS["and"]: r[rd] = r[ra] & r[rb]
        elif op == OPS["or"]: r[rd] = r[ra] | r[rb]
        elif op == OPS["xor"]: r[rd] = r[ra] ^ r[rb]
        elif op == OPS["cmpeq"]: zero = r[ra] == r[rb]
        elif op == OPS["jmp"]: next_pc = value
        elif op == OPS["jz"] and zero: next_pc = value
        elif op == OPS["jnz"] and not zero: next_pc = value
        elif op in {OPS["load8"], OPS["store8"]}:
            address = (r[ra] + value) & MASK
            if address >= RAM: raise Error(f"memory fault at 0x{address:x}")
            if op == OPS["load8"]: r[rd] = mem[address]
            else: mem[address] = r[rb] & 0xff
        elif op == OPS["out"]:
            if value != 0: raise Error(f"unsupported output port {value}")
            output.append(r[ra] & 0xff)
        elif op == OPS["halt"]:
            return value & 0xffffffff, bytes(output)
        pc = next_pc
    raise Error("execution step limit exceeded")

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source", type=Path)
    p.add_argument("--assemble", type=Path, metavar="OUTPUT")
    p.add_argument("--smoke", action="store_true")
    args = p.parse_args()
    try:
        image = assemble(args.source.read_text())
        if args.assemble:
            args.assemble.parent.mkdir(parents=True, exist_ok=True)
            args.assemble.write_bytes(image); print(args.assemble); return 0
        code, output = run(image)
        sys.stdout.buffer.write(output); sys.stdout.flush()
        if args.smoke:
            if code or b"GENERIC x128: READY" not in output:
                raise Error(f"smoke failed (guest exit={code})")
            print("x128 smoke PASSED: 128-bit ALU, branch, memory and console")
            return 0
        return code
    except (OSError, Error) as exc:
        print(f"x128: {exc}", file=sys.stderr); return 1

if __name__ == "__main__":
    raise SystemExit(main())
