#!/usr/bin/env python3
"""Independent verifier for IFC v2 round-2 findings. Sequential (one pin run at a time)."""
import json, os, subprocess, sys, time, shutil

HOME = os.path.expanduser("~")
BASE = os.path.join(HOME, ".cache/anubis-review-esc/ifc2-r2/verify")
PINS = os.path.join(HOME, ".cache/anubis-item21/pins")
CAPPED = os.path.join(HOME, ".cache/anubis-item21/capped.sh")
NEW = os.path.join(PINS, "anubis-ifc2-v8e")
LANES = os.path.join(PINS, "anubis-ord3x4l")
RUNNERS = [("NEW", NEW), ("whole10", os.path.join(PINS, "anubis-whole10")),
           ("ordret2a", os.path.join(PINS, "anubis-ordret2a")), ("LANES", LANES)]
NOISE = ("warning[ANUBIS_UNVERIFIED_EXECUTION]", "anubis run: compiling native binary", "anubis run: compile done")


def capped(cap, tmo, args, cwd=None, stdin=None):
    cmd = ["timeout", str(tmo), CAPPED] + args
    env = dict(os.environ, CAP=cap)
    t0 = time.time()
    p = subprocess.run(cmd, cwd=cwd, env=env, input=(stdin or "").encode(), capture_output=True)
    return p.returncode, p.stdout.decode(errors="replace"), p.stderr.decode(errors="replace"), time.time() - t0


def clean(s):
    return "\n".join(l for l in s.splitlines() if not l.startswith(NOISE))


def is_check_refusal(rc, out, err):
    txt = out + err
    return rc != 0 and "run failed" not in txt and "panicked" not in txt and ("Error: ANUBIS_" in txt or "error[" in txt or "Error:" in txt) and "compile done" not in txt


def run_variant(runner, path, f, tag):
    d = os.path.join(BASE, "run", f["id"], tag)
    if os.path.exists(d):
        shutil.rmtree(d)
    os.makedirs(d)
    for name, content in (f.get("setup") or {}).items():
        open(os.path.join(d, name), "w").write(content)
    rc, out, err, dt = capped("2G", 90, [runner, "run", "--no-verify", path], cwd=d, stdin=f.get("stdin"))
    return {"rc": rc, "stdout": out, "stderr": clean(err), "t": round(dt, 2)}


def main():
    files = sys.argv[1:]
    only = os.environ.get("ONLY")
    res_path = os.path.join(BASE, "results.json")
    results = json.load(open(res_path)) if os.path.exists(res_path) else {}
    os.makedirs(os.path.join(BASE, "p"), exist_ok=True)
    for fn in files:
        for f in json.load(open(fn)):
            if only and f["id"] not in only.split(","):
                continue
            pa = os.path.join(BASE, "p", f["id"] + ".anb")
            open(pa, "w").write(f["program"])
            pb = None
            if f.get("sub"):
                old, new = f["sub"]
                assert f["program"].count(old) == 1, (f["id"], old)
                pb = os.path.join(BASE, "p", f["id"] + "_b.anb")
                open(pb, "w").write(f["program"].replace(old, new))
            r = {"id": f["id"], "kind": f["kind"]}
            tmo = int(os.environ.get("IFC_TMO", "60"))
            rc, out, err, dt = capped("1500M", tmo, [NEW, "ifc2-report", pa])
            r["new_rc"], r["new_out"], r["new_t"] = rc, (out + err)[-3000:], round(dt, 2)
            rc2, out2, err2, dt2 = capped("1500M", 60, [LANES, "check", pa])
            r["lanes_rc"], r["lanes_out"], r["lanes_t"] = rc2, (out2 + err2)[-2000:], round(dt2, 2)
            # runtime
            r["runs"] = {}
            for name, runner in RUNNERS:
                a = run_variant(runner, pa, f, name + "_A")
                if is_check_refusal(a["rc"], a["stdout"], a["stderr"]):
                    r["runs"][name] = {"refused": (a["stdout"] + a["stderr"])[-600:]}
                    continue
                b = run_variant(runner, pb, f, name + "_B") if pb else None
                r["runs"][name] = {"A": a, "B": b}
                r["runner"] = name
                break
            results[f["id"]] = r
            json.dump(results, open(res_path, "w"), indent=1)
            ra = r["runs"].get(r.get("runner"), {})
            same = None
            if ra.get("B"):
                same = (ra["A"]["stdout"], ra["A"]["stderr"], ra["A"]["rc"]) == (ra["B"]["stdout"], ra["B"]["stderr"], ra["B"]["rc"])
            print(f"{f['id']}: new_rc={rc} ({dt:.2f}s) lanes_rc={rc2} runner={r.get('runner')} same={same}", flush=True)


if __name__ == "__main__":
    main()
