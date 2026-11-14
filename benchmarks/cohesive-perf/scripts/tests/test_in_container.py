# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code): tests of
# the in-container helpers on synthetic inputs: fnte_sites.py
# (fail-closed classification, x86_64 and aarch64 disassembly),
# build_record.py (ldd parsing, run-time verification) and the shell
# validators of validate.sh (through tests/test_validate.sh).

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "in-container"))
import build_record  # noqa: E402
import fnte_sites  # noqa: E402

SYM = fnte_sites.SYMBOL
NEW = "void akantu::Mesh::sendEvent<akantu::NewElementsEvent>(akantu::NewElementsEvent&)"
REM = "void akantu::Mesh::sendEvent<akantu::RemovedElementsEvent>(akantu::RemovedElementsEvent&)"
CONTACT = ("akantu::ContactDetector::search(akantu::Array<akantu::ContactElement, false>&, "
           "akantu::Array<double, true>&)")
SELECTOR = "akantu::CohesiveSurfaceSelector::onNodesAdded(akantu::Array<int, true> const&)"

AARCH64 = f"""
0000000000290e00 <_ZN6akantu4Mesh9sendEventINS_16NewElementsEventEEEvRT_>:
  290ebc:\tbl\t1f0040 <{SYM}@plt>
  290ec0:\tldr\tx0, [sp, #16]
  518e60:\tmov\tx0, x19
  518e64:\tb\t1f0040 <{SYM}@plt>
  5c1764:\tbl\t1f0040 <{SYM}@plt>
  5c1768:\tnop
"""

X86_64 = f"""
0000000000290e00 <foo>:
  290eb7:\tcall   1f0040 <{SYM}@plt>
  290ebc:\tmov    0x10(%rsp),%rdi
  2a0000:\tnotrack call 1f0040 <{SYM}@plt>
  2a0010:\tjmp    1f0040 <{SYM}@plt>
  2b0000:\tcall   1f0040 <{SYM}@plt>
  2b0005:\tnop
"""


class FnteSites(unittest.TestCase):
    def test_aarch64_listing(self):
        _, calls, tails = fnte_sites.ARCHES[183]
        rows = fnte_sites.parse_calls(AARCH64, calls, tails)
        self.assertEqual(rows, [(0x290ebc, "call", 0x290ec0), (0x518e64, "tail", None),
                                (0x5c1764, "call", 0x5c1768)])
        event, text = fnte_sites.build_table(rows, [NEW, SELECTOR, REM], "aarch64")
        self.assertEqual(event, [0x290ec0, 0x5c1768])
        self.assertEqual(len(text), 3)

    def test_x86_64_return_address_is_next_instruction(self):
        _, calls, tails = fnte_sites.ARCHES[62]
        listing = X86_64.replace("notrack call", "call  ")
        rows = fnte_sites.parse_calls(listing, calls, tails)
        self.assertEqual(rows, [(0x290eb7, "call", 0x290ebc), (0x2a0000, "call", 0x2a0010),
                                (0x2a0010, "tail", None), (0x2b0000, "call", 0x2b0005)])
        event, _ = fnte_sites.build_table(rows, [NEW, REM, SELECTOR, CONTACT], "x86_64")
        self.assertEqual(event, [0x290ebc, 0x2a0010])

    def test_unknown_reference_mnemonic_fails(self):
        _, calls, tails = fnte_sites.ARCHES[62]
        with self.assertRaisesRegex(fnte_sites.SiteError, "unexpected reference"):
            fnte_sites.parse_calls(X86_64, calls, tails)  # "notrack" prefix: not a plain call
        with self.assertRaisesRegex(fnte_sites.SiteError, "unexpected reference"):
            fnte_sites.parse_calls(f"  10:\tb.ne\t20 <{SYM}@plt>\n  14:\tnop\n",
                                   *fnte_sites.ARCHES[183][1:])
        # a direct call to the definition bypasses the PLT: not interposable
        with self.assertRaisesRegex(fnte_sites.SiteError, "unexpected reference"):
            fnte_sites.parse_calls(f"  10:\tcall   40 <{SYM}>\n  15:\tnop\n", calls, tails)
        # the PLT stub's own GOT jump (annotated by x86_64 objdump) is not a site
        stub = (f"00000000001f0040 <{SYM}@plt>:\n  1f0040:\tjmp    *0x2fe2(%rip)        "
                f"# 1f3028 <{SYM}@Base>\n  1f0046:\tpush   $0x5\n")
        self.assertEqual(fnte_sites.parse_calls(stub + X86_64.split("2a0000")[0], calls, tails),
                         [(0x290eb7, "call", 0x290ebc)])
        # local branches inside the function's own body are fine
        body = f"0000000000000040 <{SYM}>:\n  40:\tjmp    60 <{SYM}+0x20>\n  45:\tret\n"
        self.assertEqual(fnte_sites.parse_calls(body, calls, tails), [])

    def test_unexpected_or_unresolved_callers_fail(self):
        rows = [(0x10, "call", 0x14), (0x20, "call", 0x24)]
        for names, msg in (([NEW, "akantu::Mesh::read(std::string const&)"], "unexpected caller"),
                           ([NEW, "??"], "unresolved"),
                           ([CONTACT, SELECTOR], "no sendEvent call site"),
                           ([NEW], "addr2line returned")):
            with self.assertRaisesRegex(fnte_sites.SiteError, msg):
                fnte_sites.build_table(rows, names, "aarch64")

    def test_tail_call_from_event_instance_fails(self):
        rows = [(0x10, "call", 0x14), (0x20, "tail", None)]
        with self.assertRaisesRegex(fnte_sites.SiteError, "tail call"):
            fnte_sites.build_table(rows, [NEW, REM], "aarch64")

    def test_classification_is_exact(self):
        self.assertEqual(fnte_sites.classify(NEW), "event")
        self.assertEqual(fnte_sites.classify(REM), "event")
        self.assertEqual(fnte_sites.classify(CONTACT), "direct")
        with self.assertRaises(fnte_sites.SiteError):
            fnte_sites.classify("void akantu::Mesh::sendEvent<akantu::NewNodesEvent>"
                                "(akantu::NewNodesEvent&)")
        with self.assertRaises(fnte_sites.SiteError):
            fnte_sites.classify("akantu::MySurfaceSelector::onNodesAdded()")

    def test_elf_machine(self):
        hdr = bytearray(64)
        hdr[:6] = b"\x7fELF\x02\x01"
        hdr[18:20] = (62).to_bytes(2, "little")
        self.assertEqual(fnte_sites.elf_machine(bytes(hdr))[0], "x86_64")
        hdr[18:20] = (183).to_bytes(2, "little")
        self.assertEqual(fnte_sites.elf_machine(bytes(hdr))[0], "aarch64")
        hdr[18:20] = (40).to_bytes(2, "little")
        with self.assertRaises(fnte_sites.SiteError):
            fnte_sites.elf_machine(bytes(hdr))


LDD = """\tlinux-vdso.so.1 (0x0000ffff9c0d4000)
\tlibakantu.so.5 => /build/install-strict/lib/libakantu.so.5 (0x0000ffff9a000000)
\tlibmpi.so.40 => /lib/aarch64-linux-gnu/libmpi.so.40 (0x0000ffff99e00000)
\t/lib/ld-linux-aarch64.so.1 (0x0000ffff9c097000)
"""


class BuildRecord(unittest.TestCase):
    def test_parse_ldd(self):
        self.assertEqual(build_record.parse_ldd(LDD), [
            ("libakantu.so.5", "/build/install-strict/lib/libakantu.so.5"),
            ("libmpi.so.40", "/lib/aarch64-linux-gnu/libmpi.so.40"),
            ("ld-linux-aarch64.so.1", "/lib/ld-linux-aarch64.so.1")])
        with self.assertRaises(build_record.RecordError):
            build_record.parse_ldd("\tlibx.so => not found\n")

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = self.tmp.name
        self.binary = os.path.join(t, "cohesive_extrinsic")
        self.lib = os.path.join(t, "build", "libakantu.so.5")
        os.makedirs(os.path.dirname(self.lib))
        for p, data in ((self.binary, b"exe"), (self.lib, b"lib")):
            with open(p, "wb") as f:
                f.write(data)
        self.saved = (build_record.MUTABLE_PREFIXES, build_record.resolved_libraries)
        build_record.MUTABLE_PREFIXES = (os.path.join(os.path.realpath(t), "build") + "/",)
        build_record.resolved_libraries = lambda b: [("libakantu.so.5", self.lib),
                                                     ("libc.so.6", "/lib/libc.so.6")]
        real = os.path.realpath(self.lib)
        self.rec = {"binary": {"path": self.binary, "sha256": hashlib.sha256(b"exe").hexdigest()},
                    "image_id": "sha256:img",
                    "libakantu": {"sha256": hashlib.sha256(b"lib").hexdigest(), "build_id": "ab"},
                    "sparse_solver": {"auto": "mumps"},
                    "libraries": [{"soname": "libakantu.so.5", "path": self.lib, "realpath": real,
                                   "sha256": hashlib.sha256(b"lib").hexdigest()},
                                  {"soname": "libc.so.6", "path": "/lib/libc.so.6",
                                   "realpath": "/lib/libc.so.6", "sha256": "cc"}]}

    def tearDown(self):
        build_record.MUTABLE_PREFIXES, build_record.resolved_libraries = self.saved
        self.tmp.cleanup()

    def test_verify_accepts_the_recorded_build(self):
        out = build_record.verify(self.rec, "sha256:img")
        self.assertEqual(out["build_record_sha256"], build_record.digest(self.rec))
        self.assertEqual(list(out["mutable_libraries_sha256"]), [self.lib])

    def test_verify_rejects_a_rebuilt_library(self):
        with open(self.lib, "wb") as f:
            f.write(b"rebuilt")
        with self.assertRaisesRegex(build_record.RecordError, "differs from the build record"):
            build_record.verify(self.rec, "sha256:img")

    def test_verify_rejects_another_binary_or_image(self):
        with self.assertRaisesRegex(build_record.RecordError, "image id"):
            build_record.verify(self.rec, "sha256:other")
        with open(self.binary, "wb") as f:
            f.write(b"exe2")
        with self.assertRaisesRegex(build_record.RecordError, "changed since"):
            build_record.verify(self.rec, "sha256:img")

    def test_verify_rejects_other_resolution(self):
        build_record.resolved_libraries = lambda b: [("libakantu.so.5", "/other/libakantu.so.5")]
        with self.assertRaisesRegex(build_record.RecordError, "other libraries"):
            build_record.verify(self.rec, "sha256:img")

    def test_digest_is_canonical(self):
        a = {"b": 1, "a": [1, 2]}
        self.assertEqual(build_record.digest(a), build_record.digest(json.loads(json.dumps(a))))


class RunOne(unittest.TestCase):
    def test_early_abort_markers_match_perfcheck(self):
        sys.path.insert(0, os.path.dirname(HERE))
        import perfcheck
        with open(os.path.join(os.path.dirname(HERE), "in-container", "run-one.sh")) as f:
            text = f.read()
        for marker in perfcheck.MUMPS_MARKERS:
            self.assertIn(f'-e "{marker}"', text)


class ShellValidators(unittest.TestCase):
    def test_validate_sh(self):
        r = subprocess.run(["bash", os.path.join(HERE, "test_validate.sh")],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)


if __name__ == "__main__":
    unittest.main()
