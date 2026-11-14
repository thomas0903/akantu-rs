#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Generate fnte_sites.h: the call sites of Mesh::fillNodesToElements in an
installed libakantu.so that the JOSS patch removes.

    python3 -I fnte_sites.py <libakantu.so> <out.h> <out.txt>

The JOSS runs (performance-testing-cohesive, akantu-v4.0.1.patch and
akantu-v5.0.4.patch) comment out `this->fillNodesToElements()` in
`Mesh::sendEvent<NewElementsEvent>` and `Mesh::sendEvent<RemovedElementsEvent>`
(src/mesh/mesh_inline_impl.hh:75,84 at the reference commit). Those inline
specializations are instantiated in many functions of the library. In the
reference sources the only *direct* callers of fillNodesToElements are
contact-mechanics functions (contact_detector.cc:78 ContactDetector::search,
surface_selector.cc:113 CohesiveSurfaceSelector::onNodesAdded, :187
AllSurfaceSelector::onNodesAdded); every other call site is an instance of
one of the two sendEvent specializations. This script lists every call (and
tail call) to the PLT entry of fillNodesToElements and classifies it by its
innermost (inlined) enclosing function (addr2line, DWARF).

Fail closed: every site must resolve to one of the two
sendEvent instances (an "event" site, skipped in JOSS mode) or to one of the
three named direct callers (kept); an unresolved name (`??`), any other
caller, a tail call from an event instance, or no event site at all is an
error, so a different library cannot silently change what JOSS mode skips.
The table is bound to the analysed library: its SHA-256 and GNU build-id are
written to the header (the interposer refuses to run against a library with
another build-id) and to out.txt (recorded in the build record; run-one.sh
re-checks the library digest before every run).

Architectures: aarch64 (`bl`/`b`) and x86_64 (`call`/`jmp`). The recorded
offset of an event site is its return address (the address of the
instruction after the call, read from the disassembly), as seen by
`__builtin_return_address(0)` in the interposer.
"""

import hashlib
import re
import subprocess
import sys

SYMBOL = "_ZN6akantu4Mesh19fillNodesToElementsEi"
EVENT_RE = re.compile(
    r"^void akantu::Mesh::sendEvent<akantu::(NewElementsEvent|RemovedElementsEvent)>"
    r"\(akantu::\1&\)$")
DIRECT_CALLERS = (  # contact mechanics, kept in both modes
    "akantu::ContactDetector::search(",
    "akantu::CohesiveSurfaceSelector::onNodesAdded(",
    "akantu::AllSurfaceSelector::onNodesAdded(",
)
# ELF e_machine -> (architecture, call mnemonics, tail-call mnemonics)
ARCHES = {
    183: ("aarch64", ("bl",), ("b",)),
    62: ("x86_64", ("call", "callq"), ("jmp", "jmpq")),
}


class SiteError(Exception):
    pass


def elf_machine(data):
    """(architecture, call mnemonics, tail mnemonics) from an ELF header."""
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise SiteError("not a little-endian ELF64 file")
    machine = int.from_bytes(data[18:20], "little")
    if machine not in ARCHES:
        raise SiteError(f"unsupported ELF machine {machine}")
    return ARCHES[machine]


def parse_calls(disassembly, calls, tails):
    """Calls of SYMBOL@plt in `objdump -d --no-show-raw-insn` output.

    Returns [(address, kind, return_address)], kind "call" or "tail"; the
    return address of a call is the address of the next instruction line.
    Any other reference to the symbol itself (`<SYM>`, `<SYM@...>`: a
    conditional branch, a prefixed call, a direct call bypassing the PLT,
    which the interposer could not see) is an error. Local branches inside
    the function's own body (`<SYM+0x..>`) are not references.
    """
    insn = re.compile(r"^\s*([0-9a-f]+):\s+(\S+)(?:\s+(.*))?$")
    target = re.compile(r"^[0-9a-f]+ <%s@plt>$" % re.escape(SYMBOL))
    reference = re.compile(r"<%s(@[^>+]*)?>" % re.escape(SYMBOL))
    header = re.compile(r"^[0-9a-f]+ <(.*)>:$")
    rows = []
    pending = None
    in_plt_stub = False
    for line in disassembly.splitlines():
        h = header.match(line.strip())
        if h:
            # PLT stubs (x86_64 objdump annotates their GOT load with the
            # symbol name) are not call sites
            in_plt_stub = h.group(1).endswith("@plt")
            continue
        m = insn.match(line)
        if not m:
            if reference.search(line):
                raise SiteError(f"unparsed reference to {SYMBOL}: {line.strip()}")
            continue
        addr, mnemonic, operands = int(m.group(1), 16), m.group(2), (m.group(3) or "").strip()
        if pending is not None:
            rows.append((pending, "call", addr))
            pending = None
        if in_plt_stub or not reference.search(operands):
            continue
        if target.match(operands) and mnemonic in calls:
            pending = addr
        elif target.match(operands) and mnemonic in tails:
            rows.append((addr, "tail", None))
        else:
            raise SiteError(f"unexpected reference to {SYMBOL} at {addr:#x}: {line.strip()}")
    if pending is not None:
        raise SiteError(f"call at {pending:#x} is the last instruction of the listing")
    return rows


def classify(function):
    """'event', 'direct' or raise SiteError for an enclosing function name."""
    if EVENT_RE.match(function):
        return "event"
    if function.startswith(DIRECT_CALLERS):
        return "direct"
    raise SiteError(f"unexpected caller of {SYMBOL}: {function!r}")


def build_table(rows, names, arch):
    """Classify the call sites; returns (event return addresses, text rows)."""
    if len(names) != len(rows):
        raise SiteError("addr2line returned %d names for %d sites" % (len(names), len(rows)))
    event, text = [], []
    for (addr, kind, ret), fn in zip(rows, names):
        if fn.startswith("??"):
            raise SiteError(f"unresolved enclosing function at {addr:#x}")
        cls = classify(fn)
        if kind == "tail" and cls != "direct":
            raise SiteError(f"tail call to {SYMBOL} from {fn} at {addr:#x}")
        if kind == "call" and arch == "aarch64" and ret != addr + 4:
            raise SiteError(f"aarch64 bl at {addr:#x} not followed by the next instruction")
        if cls == "event":
            event.append(ret)
            label = "sendEvent instance (JOSS: removed)"
        else:
            label = "direct (kept)"
        text.append(f"{addr:#x} {kind} ret={ret:#x} {label} {fn}" if ret is not None
                    else f"{addr:#x} {kind} {label} {fn}")
    if not event:
        raise SiteError(f"no sendEvent call site of {SYMBOL} found")
    if len(set(event)) != len(event):
        raise SiteError("duplicate event return addresses")
    return sorted(event), text


def build_id(lib):
    out = subprocess.run(["readelf", "--notes", lib], check=True, capture_output=True,
                         text=True).stdout
    m = re.search(r"Build ID:\s*([0-9a-f]+)", out)
    if not m:
        raise SiteError(f"{lib} has no GNU build-id: cannot bind the site table to it")
    return m.group(1)


def main():
    lib, out_h, out_txt = sys.argv[1:4]
    data = open(lib, "rb").read()
    arch, calls, tails = elf_machine(data)
    digest = hashlib.sha256(data).hexdigest()
    bid = build_id(lib)
    dis = subprocess.run(["objdump", "-d", "--no-show-raw-insn", lib], check=True,
                         capture_output=True, text=True).stdout
    rows = parse_calls(dis, calls, tails)
    if not rows:
        raise SiteError(f"no call to {SYMBOL}@plt found in {lib}")
    names = subprocess.run(["addr2line", "-f", "-C", "-e", lib] + [hex(a) for a, _, _ in rows],
                           check=True, capture_output=True, text=True).stdout.splitlines()[0::2]
    event, text = build_table(rows, names, arch)
    with open(out_txt, "w") as f:
        f.write(f"# {lib}: calls of {SYMBOL}\n")
        f.write(f"# arch={arch} sha256={digest} build_id={bid}\n")
        f.write("\n".join(text) + "\n")
    with open(out_h, "w") as f:
        f.write("// Generated by fnte_sites.py from %s -- do not edit.\n" % lib)
        f.write("#pragma once\n#include <cstdint>\n")
        f.write(f'static const char akantu_rs_fnte_library_build_id[] = "{bid}";\n')
        f.write(f'static const char akantu_rs_fnte_library_sha256[] = "{digest}";\n')
        f.write("static const std::uintptr_t akantu_rs_fnte_event_sites[] = {\n")
        f.write("".join(f"    {a:#x},\n" for a in event))
        f.write("};\n")
    print(f"{arch}: {len(event)} event call sites, {len(text) - len(event)} kept",
          file=sys.stderr)


if __name__ == "__main__":
    try:
        main()
    except (SiteError, OSError, subprocess.CalledProcessError) as e:
        sys.exit(f"fnte_sites.py: {e}")
