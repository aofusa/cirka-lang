#!/usr/bin/env python3
"""Imports the compatibility corpus (FEAT-0119, 10-testing.md section 4).

Copies programs from a checkout of GHC's testsuite (testsuite/tests) and of
nofib into tests/achc/compat/{ghc,nofib}/, keeping each source's license
next to the files and a provenance comment at the top of every module.

    scripts/import-compat.py --ghc <ghc>/testsuite/tests --ghc-license <ghc>/LICENSE
    scripts/import-compat.py --nofib <nofib>      # into _build/compat/nofib

nofib has no single license (programs by many authors, several without
stated terms), so its programs are not committed: they are imported into
the git-ignored _build/compat/nofib and measured when present.

Selection (the corpus is a sample, not the whole testsuite):
  * GHC: the `should_run` tests of the areas in GHC_AREAS whose all.T entry
    is a plain `compile_and_run` (one module, an expected stdout, optional
    stdin / exit code / arguments), and the multi-module programs of
    `programs/` (`multimod_compile_and_run('Main', '')`).
  * nofib: the programs of NOFIB_AREAS with an expected stdout. The
    arguments and stdin are nofib's "fast" ones (FAST_OPTS / .faststdin).
Tests that need C files, Cmm, extra source files, GHC flags or other ways
are skipped: they measure GHC rather than the language.
"""

import argparse
import os
import re
import shutil
import sys

GHC_AREAS = [
    "codeGen/should_run",
    "typecheck/should_run",
    "deriving/should_run",
    "numeric/should_run",
    "simplCore/should_run",
]
NOFIB_AREAS = ["imaginary", "spectral"]
GHC_VERSION = "GHC 9.4.7"

SKIP_OPTS = [
    "extra_files",
    "extra_hc_opts",
    "pre_cmd",
    "cmd_prefix",
    "only_ways",
    "req_",
    "js_",
    "when(arch",
    "unless(arch",
    "when(opsys",
    "unless(opsys",
    "expect_broken",
    "omit_ways(['normal'",
    "ignore_stdout",
]


def t_entries(path):
    """The test(...) entries of an all.T file: (name, setup, func, args)."""
    text = open(path, encoding="utf-8", errors="replace").read()
    out = []
    i = 0
    while True:
        j = text.find("test(", i)
        if j < 0:
            break
        if j > 0 and (text[j - 1].isalnum() or text[j - 1] == "_"):
            i = j + 5
            continue
        # Balanced parentheses.
        depth = 0
        k = j + 4
        in_str = None
        while k < len(text):
            c = text[k]
            if in_str:
                if c == in_str:
                    in_str = None
            elif c in "'\"":
                in_str = c
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        body = text[j + 5 : k]
        m = re.match(r"\s*['\"]([^'\"]+)['\"]\s*,(.*)", body, re.S)
        if m:
            out.append((m.group(1), m.group(2)))
        i = k
    return out


def header(source, lic):
    return f"-- Imported from {source} ({lic}; see LICENSE in this directory).\n"


def copy_module(src, dst, provenance):
    text = open(src, encoding="utf-8", errors="replace").read()
    with open(dst, "w", encoding="utf-8") as f:
        if dst.endswith(".lhs"):
            f.write(provenance.replace("-- ", "", 1) + "\n")
        else:
            f.write(provenance)
        f.write(text)


def stdout_of(d, name):
    for ext in (".stdout-ws-64", ".stdout"):
        p = os.path.join(d, name + ext)
        if os.path.exists(p):
            return p
    return None


def import_ghc(tests, license_path, out):
    count = 0
    for area in GHC_AREAS:
        d = os.path.join(tests, area)
        allt = os.path.join(d, "all.T")
        if not os.path.exists(allt):
            continue
        dst_dir = os.path.join(out, area.split("/")[0])
        os.makedirs(dst_dir, exist_ok=True)
        for name, rest in t_entries(allt):
            if "compile_and_run" not in rest or "multimod" in rest:
                continue
            if any(s in rest for s in SKIP_OPTS):
                continue
            # The compile flags (the last list) must be empty.
            m = re.search(r"compile_and_run\s*,\s*\[\s*(['\"])(.*?)\1\s*\]", rest)
            if not m or m.group(2).strip():
                continue
            src = os.path.join(d, name + ".hs")
            if not os.path.exists(src):
                continue
            so = stdout_of(d, name)
            if not so:
                continue
            prov = header(f"{GHC_VERSION} testsuite/tests/{area}/{name}.hs", "BSD-3-Clause")
            copy_module(src, os.path.join(dst_dir, name + ".hs"), prov)
            shutil.copy(so, os.path.join(dst_dir, name + ".stdout"))
            si = os.path.join(d, name + ".stdin")
            if os.path.exists(si):
                shutil.copy(si, os.path.join(dst_dir, name + ".stdin"))
            ec = re.search(r"exit_code\(\s*(\d+)\s*\)", rest)
            if ec and ec.group(1) != "0":
                open(os.path.join(dst_dir, name + ".exitcode"), "w").write(ec.group(1) + "\n")
            ro = re.search(r"extra_run_opts\(\s*(['\"])(.*?)\1\s*\)", rest)
            if ro:
                opts = ro.group(2)
                if "<" in opts or ">" in opts or "+RTS" in opts:
                    os.remove(os.path.join(dst_dir, name + ".hs"))
                    os.remove(os.path.join(dst_dir, name + ".stdout"))
                    for e in (".stdin", ".exitcode"):
                        p = os.path.join(dst_dir, name + e)
                        if os.path.exists(p):
                            os.remove(p)
                    continue
                open(os.path.join(dst_dir, name + ".args"), "w").write(opts + "\n")
            count += 1
        shutil.copy(license_path, os.path.join(dst_dir, "LICENSE"))
    # Multi-module programs.
    pdir = os.path.join(tests, "programs")
    dst_root = os.path.join(out, "programs")
    for prog in sorted(os.listdir(pdir)) if os.path.isdir(pdir) else []:
        d = os.path.join(pdir, prog)
        allt = os.path.join(d, "test.T")
        if not os.path.exists(allt):
            continue
        entries = t_entries(allt)
        if len(entries) != 1:
            continue
        name, rest = entries[0]
        if not re.search(r"multimod_compile_and_run\s*,\s*\[\s*'Main'\s*,\s*''\s*\]", rest):
            continue
        if any(s in rest for s in ["extra_hc_opts", "pre_cmd", "expect_broken", "extra_run_opts"]):
            continue
        so = stdout_of(d, name)
        mains = [f for f in ("Main.hs", "Main.lhs") if os.path.exists(os.path.join(d, f))]
        if not so or not mains:
            continue
        dst = os.path.join(dst_root, prog)
        os.makedirs(dst, exist_ok=True)
        for f in sorted(os.listdir(d)):
            if f.endswith((".hs", ".lhs")) and not f.endswith("-boot"):
                prov = header(f"{GHC_VERSION} testsuite/tests/programs/{prog}/{f}", "BSD-3-Clause")
                copy_module(os.path.join(d, f), os.path.join(dst, f), prov)
        shutil.copy(so, os.path.join(dst, "Main.stdout"))
        si = os.path.join(d, name + ".stdin")
        if os.path.exists(si):
            shutil.copy(si, os.path.join(dst, "Main.stdin"))
        ec = re.search(r"exit_code\(\s*(\d+)\s*\)", rest)
        if ec and ec.group(1) != "0":
            open(os.path.join(dst, "Main.exitcode"), "w").write(ec.group(1) + "\n")
        count += 1
    if os.path.isdir(dst_root):
        shutil.copy(license_path, os.path.join(dst_root, "LICENSE"))
    return count


def make_var(text, var):
    m = re.search(rf"^{var}\s*[+:]?=\s*(.*)$", text, re.M)
    return m.group(1).strip() if m else None


def import_nofib(nofib, out):
    count = 0
    for area in NOFIB_AREAS:
        adir = os.path.join(nofib, area)
        if not os.path.isdir(adir):
            continue
        for prog in sorted(os.listdir(adir)):
            d = os.path.join(adir, prog)
            mk = os.path.join(d, "Makefile")
            if not os.path.exists(mk):
                continue
            mtext = open(mk, encoding="utf-8", errors="replace").read()
            if re.search(r"^SUBDIRS", mtext, re.M):
                continue
            srcs = [f for f in sorted(os.listdir(d)) if f.endswith((".hs", ".lhs"))]
            if not srcs or not any(f.startswith("Main.") for f in srcs):
                continue
            so = None
            for ext in (".faststdout", ".stdout"):
                p = os.path.join(d, prog + ext)
                if os.path.exists(p):
                    so = p
                    break
            if not so:
                continue
            if make_var(mtext, "SRC_HC_OPTS") and "-package" in (make_var(mtext, "SRC_HC_OPTS") or ""):
                continue
            args = make_var(mtext, "FAST_OPTS") or make_var(mtext, "PROG_ARGS") or ""
            if "$(" in args or "+RTS" in args or "<" in args:
                continue
            si = None
            for ext in (".faststdin", ".stdin"):
                p = os.path.join(d, prog + ext)
                if os.path.exists(p):
                    si = p
                    break
            dst = os.path.join(out, area, prog)
            os.makedirs(dst, exist_ok=True)
            for f in srcs:
                prov = f"-- Imported from nofib {area}/{prog}/{f} (not redistributed).\n"
                copy_module(os.path.join(d, f), os.path.join(dst, f), prov)
            shutil.copy(so, os.path.join(dst, "Main.stdout"))
            if si:
                shutil.copy(si, os.path.join(dst, "Main.stdin"))
            if args:
                open(os.path.join(dst, "Main.args"), "w").write(args + "\n")
            rf = os.path.join(d, "runtime_files")
            if os.path.isdir(rf):
                shutil.copytree(rf, os.path.join(dst, "runtime_files"))
            count += 1
    return count


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--ghc", help="GHC's testsuite/tests directory")
    ap.add_argument("--ghc-license", help="GHC's LICENSE file")
    ap.add_argument("--nofib", help="a nofib checkout")
    ap.add_argument("--out", default=os.path.join(os.path.dirname(__file__), "..", "tests", "achc", "compat"))
    a = ap.parse_args()
    if a.ghc:
        if not a.ghc_license:
            sys.exit("--ghc-license is required with --ghc")
        out = os.path.join(a.out, "ghc")
        shutil.rmtree(out, ignore_errors=True)
        print("ghc:", import_ghc(a.ghc, a.ghc_license, out), "programs")
    if a.nofib:
        out = os.path.join(os.path.dirname(__file__), "..", "_build", "compat", "nofib")
        shutil.rmtree(out, ignore_errors=True)
        n = import_nofib(a.nofib, out)
        print("nofib:", n, "programs (in _build/compat/nofib, not committed)")


if __name__ == "__main__":
    main()
