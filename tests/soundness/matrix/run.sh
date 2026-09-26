#!/usr/bin/env bash
# Soundness acceptance matrix runner.
#   run.sh <anubis-binary> [--out NEW-DIR] [--timeout SECONDS]
#          [--expected-registry-sha256 HEX]
#          [--expected-classes FILE --expected-classes-sha256 HEX]
#          [--record LABEL SOURCE-COMMIT]
#
# The old positional binary and --record form remain supported. Per-case timeout
# defaults to 120 seconds. All logs and an atomic JSON receipt remain in --out (or
# the printed temporary directory). Operator-supplied matching registry and
# class-expectation digests provide provisional grading only. Expectations are frozen
# independently of checker output: an entry keyed by "ID/form" contains the
# source SHA-256 and exact typed diagnostics, each with an obligation fingerprint
# or source span. Every non-ACCEPT form needs one. The supplied digests remain
# provisional until a reviewed canonical manifest is checked in: this version
# cannot report PASS or complete matrix coverage. Locally matching forms are
# marked MATCH-provisional. Current --record
# syntax remains recognized, but recording is disabled until source-bound build
# provenance and concurrent-safe history publication exist. Its receipt says so.
# Exit: INCOMPLETE=2 for every run in this provisional version.
set -euo pipefail
MATRIX_HERE=$(cd "$(dirname "$0")" && pwd)
export MATRIX_HERE
exec python3 - "$@" <<'PY'
import argparse
import collections
import hashlib
import json
import math
import os
from pathlib import Path
import re
import selectors
import signal
import stat
import subprocess
import sys
import tempfile
import time

HERE = Path(os.environ["MATRIX_HERE"])
REGISTRY = HERE / "registry.tsv"
RUNNER = HERE / "run.sh"
REGISTRY_HEADER = "id\tfamily\tcategory\tintent\tforms\tnotes"
RESULT_HEADER = ("id\tform\tcategory\tintent\tobserved\tresult\texit_code"
                 "\tsource_sha256\tstdout\tstdout_sha256\tstderr\tstderr_sha256"
                 "\ttranscript_sha256")
INTENTS = {"ACCEPT", "REJECT", "UNRES", "REJ|UNRES", "MALFORMED", "LIMIT"}
SECURITY_PREFIXES = (
    "ANUBIS_SECRET_EXFILTRATION", "ANUBIS_TAINTED_SINK", "ANUBIS_INTERPROC_",
    "ANUBIS_EFFECT_", "ANUBIS_CAPABILITY_", "ANUBIS_IMPLICIT_FLOW",
)
REFUSAL_CODES = {
    "ANUBIS_FLOAT_CONTRACT_UNMODELED", "ANUBIS_LOOP_INVARIANT_UNVERIFIABLE",
    "ANUBIS_DIVISOR_MAYBE_ZERO",
}
REJECTION_CLASSES = {"DISPROVED", "SEC_REJECT", "WRAP", "REJECTED"}
EXPECTATION_FIELDS = ("code", "family", "status", "defect_locus", "agent_action")
MAX_LOG_BYTES = 8388608
READ_CHUNK = 65536


class RunInterrupted(Exception):
    pass


def interrupted(_signum, _frame):
    raise RunInterrupted("runner interrupted")


def positive_timeout(value):
    try:
        seconds = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("timeout must be finite and positive") from error
    if not math.isfinite(seconds) or seconds <= 0:
        raise argparse.ArgumentTypeError("timeout must be finite and positive")
    return seconds


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(READ_CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def sha256_json(value):
    data = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(data).hexdigest()


def atomic_json(path, value):
    name = None
    try:
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent,
                                         prefix=".receipt-", delete=False) as stream:
            name = stream.name
            json.dump(value, stream, sort_keys=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
    finally:
        if name is not None and os.path.exists(name):
            os.unlink(name)


def load_registry():
    lines = REGISTRY.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != REGISTRY_HEADER or len(lines) == 1:
        raise ValueError("registry header is invalid or registry has no cases")
    specs, seen = [], set()
    for line_number, line in enumerate(lines[1:], start=2):
        fields = line.split("\t")
        if len(fields) != 6:
            raise ValueError(f"registry line {line_number}: expected six TSV fields")
        case_id, family, category, intent, forms, notes = fields
        if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]*", case_id):
            raise ValueError(f"registry line {line_number}: invalid case ID")
        if case_id in seen:
            raise ValueError(f"registry line {line_number}: duplicate case ID {case_id}")
        seen.add(case_id)
        if not family.strip() or not category.strip() or not notes.strip():
            raise ValueError(f"registry line {line_number}: empty required field")
        if intent not in INTENTS:
            raise ValueError(f"registry line {line_number}: unknown intent")
        if forms not in ("carrier", "carrier,direct"):
            raise ValueError(f"registry line {line_number}: invalid or repeated form")
        for form in forms.split(","):
            suffix = ".direct.anb" if form == "direct" else ".anb"
            relative = f"cases/{case_id}{suffix}"
            source = HERE / relative
            if not source.is_file() or source.is_symlink():
                raise ValueError(f"registry line {line_number}: missing or linked {relative}")
            specs.append({"id": case_id, "form": form, "category": category,
                          "intent": intent, "source": relative,
                          "source_sha256": sha256_file(source)})
    registered = {spec["source"] for spec in specs}
    found = {path.relative_to(HERE).as_posix()
             for path in (HERE / "cases").rglob("*.anb")}
    if found != registered:
        missing = sorted(registered - found)
        extra = sorted(found - registered)
        raise ValueError(f"registry/cases mismatch: missing={missing}, unregistered={extra}")
    return specs


def load_expectations(path, expected_sha, registry_sha, specs):
    if not path.is_file():
        raise ValueError("expected-classes file is missing")
    if path.stat().st_size > MAX_LOG_BYTES:
        raise ValueError("expected-classes file exceeds bounded input size")
    if sha256_file(path) != expected_sha:
        raise ValueError("expected-classes digest does not match supplied digest")
    try:
        document = json.loads(path.read_text(encoding="utf-8"),
                              object_pairs_hook=unique_object,
                              parse_constant=reject_json_constant)
    except (UnicodeError, ValueError) as error:
        raise ValueError(f"expected-classes JSON is invalid: {error}") from error
    if (not isinstance(document, dict) or
            set(document) != {"schema", "registry_sha256", "non_accept"} or
            document["schema"] != "anubis-matrix-expected-diagnostics/2" or
            document["registry_sha256"] != registry_sha or
            not isinstance(document["non_accept"], dict)):
        raise ValueError("expected-classes schema or registry binding is invalid")
    expected_specs = {f"{s['id']}/{s['form']}": s for s in specs
                      if s["intent"] != "ACCEPT"}
    if set(document["non_accept"]) - set(expected_specs):
        raise ValueError("expected-classes contains an ACCEPT or unknown form")
    expectations = {}
    for key, expectation in document["non_accept"].items():
        if (not isinstance(expectation, dict) or
                set(expectation) != {"source_sha256", "diagnostics"} or
                expectation["source_sha256"] != expected_specs[key]["source_sha256"]):
            raise ValueError(f"expected-classes {key}: source binding is invalid")
        required = expectation["diagnostics"]
        if not isinstance(required, list) or not required:
            raise ValueError(f"expected-classes {key}: expected nonempty diagnostic tuple list")
        tuples = []
        for item in required:
            if (not isinstance(item, dict) or
                    set(item) != (set(EXPECTATION_FIELDS) | {"locator"}) or
                    any(not isinstance(item[field], str) for field in EXPECTATION_FIELDS) or
                    not re.fullmatch(r"ANUBIS_[A-Z0-9_]+", item["code"])):
                raise ValueError(f"expected-classes {key}: invalid diagnostic tuple")
            locator = item["locator"]
            if not isinstance(locator, dict):
                raise ValueError(f"expected-classes {key}: missing obligation locator")
            if locator.get("kind") == "obligation":
                if (set(locator) != {"kind", "name", "smt_sha256"} or
                        not isinstance(locator["name"], str) or
                        not locator["name"] or
                        not isinstance(locator["smt_sha256"], str) or
                        not re.fullmatch(r"[0-9a-f]{64}", locator["smt_sha256"])):
                    raise ValueError(f"expected-classes {key}: invalid obligation fingerprint")
            elif locator.get("kind") == "span":
                if (set(locator) != {"kind", "file", "span_start", "span_end"} or
                        locator["file"] != expected_specs[key]["source"] or
                        not unsigned_integer(locator["span_start"]) or
                        not unsigned_integer(locator["span_end"]) or
                        locator["span_end"] < locator["span_start"] or
                        locator["span_end"] > (HERE / locator["file"]).stat().st_size):
                    raise ValueError(f"expected-classes {key}: invalid source span")
            else:
                raise ValueError(f"expected-classes {key}: unsupported obligation locator")
            tuples.append(item)
        expectations[key] = tuples
    return expectations


def stop_owned_group(process):
    # The checker starts a private session. No global PID search or broad kill.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=2)
        return True
    except subprocess.TimeoutExpired:
        return False


def owned_group_alive(process):
    try:
        os.killpg(process.pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        # A child we started should remain signalable, but inaccessible
        # descendants cannot be interpreted as successful cleanup.
        return True


def run_owned(binary, arguments, stdout_log, stderr_log, timeout):
    """Return (exit code, tool issue); retain separate bounded child streams."""
    seen, issue, process = {"stdout": 0, "stderr": 0}, None, None
    try:
        with stdout_log.open("wb") as stdout, stderr_log.open("wb") as stderr:
            try:
                process = subprocess.Popen(
                    [str(binary), *arguments], cwd=HERE, stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE, stdin=subprocess.DEVNULL,
                    start_new_session=True,
                )
            except OSError as error:
                stderr.write(f"TOOL_SPAWN_ERROR: {error.strerror}\n".encode())
                return None, "TOOL_SPAWN_ERROR"
            with selectors.DefaultSelector() as selector:
                assert process.stdout is not None and process.stderr is not None
                selector.register(process.stdout, selectors.EVENT_READ, "stdout")
                selector.register(process.stderr, selectors.EVENT_READ, "stderr")
                deadline = time.monotonic() + timeout
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        issue = "TOOL_TIMEOUT"
                        break
                    for key, _mask in selector.select(timeout=min(remaining, 0.25)):
                        data = os.read(key.fd, READ_CHUNK)
                        if not data:
                            selector.unregister(key.fileobj)
                            continue
                        channel = key.data
                        available = MAX_LOG_BYTES - seen[channel]
                        if len(data) > available:
                            (stdout if channel == "stdout" else stderr).write(data[:available])
                            issue = "TOOL_OUTPUT_LIMIT"
                            break
                        (stdout if channel == "stdout" else stderr).write(data)
                        seen[channel] += len(data)
                    if issue is not None:
                        break
            if issue is not None:
                if not stop_owned_group(process):
                    issue = "TOOL_CLEANUP_TIMEOUT"
            else:
                try:
                    process.wait(timeout=max(deadline - time.monotonic(), 0))
                    # A checker may exit while a child closed both pipes and
                    # remains in its session. Reap the owned group on normal
                    # completion as well as timeout, never by global PID scan.
                    if owned_group_alive(process):
                        issue = "TOOL_CHILD_CLEANUP"
                    if not stop_owned_group(process):
                        issue = "TOOL_CLEANUP_TIMEOUT"
                except subprocess.TimeoutExpired:
                    issue = "TOOL_TIMEOUT"
                    if not stop_owned_group(process):
                        issue = "TOOL_CLEANUP_TIMEOUT"
            for output in (stdout, stderr):
                output.flush()
                os.fsync(output.fileno())
            return process.returncode, issue
    except BaseException:
        if process is not None:
            stop_owned_group(process)
        raise
    finally:
        if process is not None:
            for pipe in (process.stdout, process.stderr):
                if pipe is not None:
                    pipe.close()


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("duplicate JSON key")
        value[key] = item
    return value


def reject_json_constant(_value):
    raise ValueError("non-finite JSON number")


def unsigned_integer(value):
    return type(value) is int and value >= 0


def diagnostic_class(diagnostic):
    code, family, status = (diagnostic[k] for k in ("code", "family", "status"))
    locus, action = (diagnostic[k] for k in ("defect_locus", "agent_action"))
    if code == "ANUBIS_ASSERTION_DISPROVED":
        return "DISPROVED" if (family, status, locus, action) == (
            "contract", "disproved", "program", "repair_program") else "TOOL_PROTOCOL"
    if code == "ANUBIS_WRAP_RISK":
        return "WRAP" if (family, status, locus, action) == (
            "wrap_safety", "disproved", "program", "repair_program") else "TOOL_PROTOCOL"
    if code == "ANUBIS_ASSERTION_UNDECIDED":
        return "UNDECIDED" if (family in ("contract", "wrap_safety") and
                               status == "undecided" and
                               locus in ("program", "capability")) else "TOOL_PROTOCOL"
    if code == "ANUBIS_ASSERTION_UNPROVEN":
        return "UNPROVEN" if (family == "contract" and status == "refused" and
                              locus in ("program", "capability")) else "TOOL_PROTOCOL"
    if code.startswith(SECURITY_PREFIXES):
        return "SEC_REJECT" if (family, status, locus, action) == (
            "frontend", "refused", "program", "repair_program") else "TOOL_PROTOCOL"
    if code in ("ANUBIS_PARSE_DEPTH_LIMIT", "ANUBIS_PARSE_CHAIN_LIMIT",
                "ANUBIS_PARSE_LIMIT"):
        return "PARSE_LIMIT" if (family, status, locus, action) == (
            "frontend", "refused", "program", "repair_program") else "TOOL_PROTOCOL"
    if code == "ANUBIS_PARSE_ERROR":
        return "PARSE" if (family, status, locus, action) == (
            "frontend", "refused", "program", "repair_program") else "TOOL_PROTOCOL"
    if code == "ANUBIS_ANALYSIS_LIMIT":
        return "ANALYSIS_LIMIT" if (family, status, locus, action) == (
            "frontend", "refused", "capability", "restate_or_raise_budget") else "TOOL_PROTOCOL"
    if code in REFUSAL_CODES:
        return "REFUSED" if (family, status, locus, action) == (
            "frontend", "refused", "program", "repair_program") else "TOOL_PROTOCOL"
    if family in ("solver_trust", "environment") or locus in ("compiler", "environment"):
        return "TOOL_REFUSAL"
    if family == "frontend" and status == "refused":
        return f"INVALID:{code}"
    return "TOOL_PROTOCOL"


def parse_jsonl(stdout_log, return_code, issue):
    if issue is not None:
        return issue, [], None
    if return_code not in (0, 1):
        return f"TOOL_EXIT:{return_code}", [], None
    try:
        raw = stdout_log.read_bytes()
        if not raw or not raw.endswith(b"\n"):
            raise ValueError("missing or unterminated JSONL")
        lines = raw[:-1].split(b"\n")
        if any(not line for line in lines):
            raise ValueError("blank JSONL line")
        objects = [json.loads(line.decode("utf-8"), object_pairs_hook=unique_object,
                              parse_constant=reject_json_constant) for line in lines]
        if any(not isinstance(obj, dict) for obj in objects):
            raise ValueError("JSONL line is not an object")
        diagnostics, summary = objects[:-1], objects[-1]
        if summary.get("$type") != "anubis.summary" or summary.get("schema") != "anubis-diagnostics/1":
            raise ValueError("missing final summary")
        if summary.get("verdict") not in ("pass", "fail"):
            raise ValueError("invalid summary verdict")
        counts = summary.get("counts")
        count_keys = ("total", "disproved", "undecided", "replay_mismatch", "refused")
        if (not isinstance(counts, dict) or set(counts) != set(count_keys) or
                any(not unsigned_integer(counts.get(k)) for k in count_keys)):
            raise ValueError("invalid summary counts")
        computed = {k: 0 for k in count_keys}
        classes = []
        for diagnostic in diagnostics:
            required = ("code", "family", "status", "defect_locus", "agent_action",
                        "severity", "build_blocking", "message", "suggestions")
            if diagnostic.get("$type") != "anubis.diagnostic" or diagnostic.get(
                    "schema") != "anubis-diagnostics/1":
                raise ValueError("unexpected JSONL object")
            if any(k not in diagnostic for k in required):
                raise ValueError("incomplete diagnostic")
            if (not isinstance(diagnostic["code"], str) or
                    not re.fullmatch(r"ANUBIS_[A-Z0-9_]+", diagnostic["code"]) or
                    diagnostic["family"] not in ("contract", "wrap_safety", "solver_trust",
                                                 "frontend", "environment") or
                    diagnostic["status"] not in ("disproved", "undecided",
                                                 "replay_mismatch", "refused") or
                    diagnostic["defect_locus"] not in ("program", "compiler",
                                                      "environment", "capability") or
                    diagnostic["agent_action"] not in ("repair_program",
                                                      "restate_or_raise_budget",
                                                      "investigate_compiler", "fix_environment") or
                    diagnostic["severity"] != "error" or
                    diagnostic["build_blocking"] is not True or
                    not isinstance(diagnostic["message"], str) or
                    diagnostic["suggestions"] != []):
                raise ValueError("invalid diagnostic field")
            omitted = diagnostic.get("omitted", 0)
            if (not unsigned_integer(omitted) or
                    (omitted > 0 and diagnostic["code"] != "ANUBIS_PARSE_ERROR")):
                raise ValueError("invalid omitted count")
            weight = omitted if omitted > 0 else 1
            computed["total"] += weight
            computed[diagnostic["status"]] += weight
            category = diagnostic_class(diagnostic)
            classes.append(category)
        if any(counts[k] != computed[k] for k in count_keys):
            raise ValueError("summary counts disagree with diagnostics")
        if (return_code == 0) != (summary["verdict"] == "pass" and
                                  not diagnostics and counts["total"] == 0):
            raise ValueError("exit code, summary and diagnostics disagree")
        if return_code == 1 and (summary["verdict"] != "fail" or not diagnostics):
            raise ValueError("failed exit without failed diagnostic summary")
        coverage = summary.get("coverage")
        if coverage is not None:
            if not isinstance(coverage, dict):
                raise ValueError("invalid coverage")
            for key in ("certified", "trusted_to_solver", "discharged", "not_discharged"):
                if not unsigned_integer(coverage.get(key)):
                    raise ValueError("invalid coverage count")
            if (coverage["certified"] + coverage["trusted_to_solver"] !=
                    coverage["discharged"] or
                    not isinstance(coverage.get("uncertified"), list) or
                    any(not isinstance(item, str) for item in coverage["uncertified"]) or
                    type(coverage.get("witnesses_retained")) is not bool):
                raise ValueError("inconsistent coverage")
            if return_code == 0 and coverage["not_discharged"] > 0:
                raise ValueError("pass has undischarged obligations")
        if return_code == 0:
            return "ACCEPT", [], summary
        if "TOOL_PROTOCOL" in classes:
            return "TOOL_PROTOCOL", diagnostics, summary
        if any(c.startswith("TOOL_") for c in classes):
            return "TOOL_REFUSAL", diagnostics, summary
        if any(c.startswith("INVALID:") for c in classes):
            return "INVALID:diagnostic", diagnostics, summary
        distinct = set(classes)
        if distinct <= {"PARSE", "PARSE_LIMIT"}:
            return "PARSE_LIMIT" if "PARSE_LIMIT" in distinct else "PARSE", diagnostics, summary
        if "PARSE" in distinct or "PARSE_LIMIT" in distinct:
            return "INVALID:mixed-parse", diagnostics, summary
        if len(distinct) == 1:
            return classes[0], diagnostics, summary
        if distinct <= REJECTION_CLASSES:
            return "REJECTED", diagnostics, summary
        return "MIXED", diagnostics, summary
    except (UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError):
        return "TOOL_PROTOCOL", [], None


def actual_locator(diagnostic, kind):
    if kind == "obligation":
        obligation = diagnostic.get("obligation")
        if (not isinstance(obligation, dict) or
                not isinstance(obligation.get("name"), str) or
                not isinstance(obligation.get("smt"), str)):
            return None
        return {
            "kind": "obligation", "name": obligation["name"],
            "smt_sha256": hashlib.sha256(obligation["smt"].encode("utf-8")).hexdigest(),
        }
    if kind == "span":
        location = diagnostic.get("location")
        if (not isinstance(location, dict) or
                not isinstance(location.get("file"), str) or
                not unsigned_integer(location.get("span_start")) or
                not unsigned_integer(location.get("span_end")) or
                not unsigned_integer(location.get("line")) or
                not unsigned_integer(location.get("column"))):
            return None
        return {
            "kind": "span", "file": location["file"],
            "span_start": location["span_start"], "span_end": location["span_end"],
        }
    return None


def compare_expected_diagnostics(expected, diagnostics):
    if len(expected) != len(diagnostics):
        if any(actual_locator(d, "obligation") is None and
               actual_locator(d, "span") is None for d in diagnostics):
            return "INCOMPLETE-obligation-unbound"
        return "FAIL-wrong-obligation"
    unused = list(diagnostics)
    for item in expected:
        base = tuple(item[field] for field in EXPECTATION_FIELDS)
        locator = item["locator"]
        if not any(tuple(d[field] for field in EXPECTATION_FIELDS) == base for d in unused):
            return "FAIL-wrong-class"
        match = next((index for index, diagnostic in enumerate(unused)
                      if tuple(diagnostic[field] for field in EXPECTATION_FIELDS) == base
                      and actual_locator(diagnostic, locator["kind"]) == locator), None)
        if match is None:
            if any(tuple(d[field] for field in EXPECTATION_FIELDS) == base and
                   actual_locator(d, locator["kind"]) is None for d in unused):
                return "INCOMPLETE-obligation-unbound"
            return "FAIL-wrong-obligation"
        unused.pop(match)
    return "MATCH-provisional"


def grade(intent, observed, expected_diagnostics, diagnostics):
    if observed.startswith("TOOL_"):
        return "INCOMPLETE-tool"
    if intent == "ACCEPT":
        if observed == "ACCEPT":
            return "MATCH-provisional"
        return "INVALID" if observed.startswith("INVALID:") else "FAIL-wrong-class"
    if observed == "ACCEPT":
        return "FAIL-silent-accept"
    if expected_diagnostics is None:
        return "INCOMPLETE-class-unbound"
    if observed.startswith("INVALID:"):
        return "INVALID"
    if intent == "MALFORMED":
        meets = observed == "PARSE"
    elif intent == "LIMIT":
        if observed == "PARSE":
            # The production parser's generic code cannot establish which
            # implementation limit fired, regardless of its English message.
            return "INCOMPLETE-class-unbound"
        meets = observed in ("PARSE_LIMIT", "ANALYSIS_LIMIT")
    elif intent == "REJECT":
        meets = observed in REJECTION_CLASSES
    elif intent == "UNRES":
        meets = observed == "UNDECIDED"
    else:
        meets = observed in (REJECTION_CLASSES | {"UNDECIDED", "UNPROVEN", "REFUSED", "MIXED"})
    if not meets:
        return "FAIL-wrong-class"
    return compare_expected_diagnostics(expected_diagnostics, diagnostics)


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description="Run the complete soundness matrix")
    parser.add_argument("binary", help="source-matched immutable executable")
    parser.add_argument("--out", help="new directory for retained logs and receipt")
    parser.add_argument("--timeout", type=positive_timeout, default=120.0,
                        metavar="SECONDS", help="finite per-check limit (default: 120)")
    parser.add_argument("--record", nargs=2, metavar=("LABEL", "SOURCE_COMMIT"))
    parser.add_argument("--expected-registry-sha256", metavar="HEX",
                        help="operator-supplied digest of the frozen complete registry")
    parser.add_argument("--expected-classes", metavar="FILE",
                        help="provisional typed diagnostic tuples for each non-ACCEPT form")
    parser.add_argument("--expected-classes-sha256", metavar="HEX",
                        help="operator-supplied SHA-256 for --expected-classes")
    args = parser.parse_args()
    if args.expected_registry_sha256 is not None and not re.fullmatch(
            r"[0-9a-fA-F]{64}", args.expected_registry_sha256):
        parser.error("expected registry SHA-256 must be 64 hexadecimal characters")
    if (args.expected_classes is None) != (args.expected_classes_sha256 is None):
        parser.error("--expected-classes and --expected-classes-sha256 must be supplied together")
    if args.expected_classes_sha256 is not None and not re.fullmatch(
            r"[0-9a-fA-F]{64}", args.expected_classes_sha256):
        parser.error("expected classes SHA-256 must be 64 hexadecimal characters")
    if args.record is not None:
        label, commit = args.record
        if not re.fullmatch(r"[A-Za-z0-9._-]+", label):
            parser.error("record label must be path-free ASCII")
        if not re.fullmatch(r"[0-9a-fA-F]{7,64}", commit):
            parser.error("source commit must be a hexadecimal revision")

    if args.out is None:
        state_root = Path(os.environ.get("XDG_STATE_HOME",
                                         str(Path.home() / ".local" / "state")))
        default_root = state_root / "anubis" / "matrix-runs"
        default_root.mkdir(mode=0o700, parents=True, exist_ok=True)
        out = Path(tempfile.mkdtemp(prefix="run-", dir=default_root))
    else:
        out = Path(args.out).expanduser()
        try:
            out.mkdir(mode=0o700)
        except OSError as error:
            parser.error(f"--out must name a new directory: {error.strerror}")
    out.chmod(0o700)
    (out / "logs").mkdir(mode=0o700)
    print(f"matrix output: {out}", flush=True)
    atomic_json(out / "receipt.json", {
        "schema": "anubis-soundness-matrix-run/1", "phase": "running",
        "status": "INCOMPLETE", "errors": ["final grading receipt not yet published"],
    })
    errors, entries, specs = [], [], []
    registry_sha = binary_sha_before = binary_sha_after = None
    classes_sha_before = classes_sha_after = None
    expectations = {}
    version_stdout_sha = version_stderr_sha = version_transcript_sha = None
    runner_sha = sha256_file(RUNNER)
    binary = Path(args.binary).expanduser().resolve()
    classes_path = (Path(args.expected_classes).expanduser().resolve()
                    if args.expected_classes is not None else None)
    try:
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise ValueError("binary is missing or not executable")
        binary_sha_before = sha256_file(binary)
        registry_sha = sha256_file(REGISTRY)
        if (args.expected_registry_sha256 is not None and
                registry_sha != args.expected_registry_sha256.lower()):
            raise ValueError("registry digest does not match the operator-supplied expected digest")
        specs = load_registry()
        if classes_path is not None:
            classes_sha_before = sha256_file(classes_path)
            expectations = load_expectations(classes_path,
                                             args.expected_classes_sha256.lower(),
                                             registry_sha, specs)
        version_stdout = out / "binary-version.stdout.log"
        version_stderr = out / "binary-version.stderr.log"
        version_rc, version_issue = run_owned(binary, ["--version"], version_stdout,
                                              version_stderr, args.timeout)
        version_stdout_sha = sha256_file(version_stdout)
        version_stderr_sha = sha256_file(version_stderr)
        version_transcript_sha = sha256_json({
            "command": ["--version"], "binary_sha256": binary_sha_before,
            "exit_code": version_rc, "tool_issue": version_issue,
            "stdout_sha256": version_stdout_sha, "stderr_sha256": version_stderr_sha,
        })
        if version_issue is not None or version_rc != 0:
            raise ValueError(f"binary version preflight failed: {version_issue or version_rc}")
        with (out / "results.tsv").open("w", encoding="utf-8", newline="") as results:
            results.write(RESULT_HEADER + "\n")
            for spec in specs:
                stdout_relative = f"logs/{spec['id']}.{spec['form']}.stdout.jsonl"
                stderr_relative = f"logs/{spec['id']}.{spec['form']}.stderr.log"
                stdout_log, stderr_log = out / stdout_relative, out / stderr_relative
                command = ["check", spec["source"], "--message-format=json"]
                rc, issue = run_owned(binary, command, stdout_log, stderr_log, args.timeout)
                observed, diagnostics, summary = parse_jsonl(stdout_log, rc, issue)
                expected_diagnostics = expectations.get(f"{spec['id']}/{spec['form']}")
                result = grade(spec["intent"], observed, expected_diagnostics, diagnostics)
                stdout_sha, stderr_sha = sha256_file(stdout_log), sha256_file(stderr_log)
                transcript_sha = sha256_json({
                    "command": command, "binary_sha256": binary_sha_before,
                    "source_sha256": spec["source_sha256"], "exit_code": rc,
                    "tool_issue": issue, "stdout_sha256": stdout_sha,
                    "stderr_sha256": stderr_sha,
                })
                entry = {**spec, "observed": observed, "result": result,
                         "exit_code": rc, "stdout": stdout_relative,
                         "stdout_sha256": stdout_sha, "stderr": stderr_relative,
                         "stderr_sha256": stderr_sha,
                         "transcript_sha256": transcript_sha,
                         "diagnostics": [{"code": d["code"], "family": d["family"],
                                          "status": d["status"],
                                          "defect_locus": d["defect_locus"],
                                          "agent_action": d["agent_action"]}
                                         for d in diagnostics],
                         "expected_diagnostics": expected_diagnostics,
                         "summary_counts": summary["counts"] if summary else None}
                entries.append(entry)
                results.write("\t".join(str(entry[field]) for field in (
                    "id", "form", "category", "intent", "observed", "result",
                    "exit_code", "source_sha256", "stdout", "stdout_sha256",
                    "stderr", "stderr_sha256", "transcript_sha256")) + "\n")
                results.flush()
            os.fsync(results.fileno())
    except (KeyboardInterrupt, RunInterrupted) as error:
        errors.append(str(error) or "runner interrupted")
    except ValueError as error:
        errors.append(str(error))
    except OSError as error:
        errors.append(f"preflight or I/O failure: {error.strerror}")

    try:
        binary_sha_after = sha256_file(binary) if binary.is_file() else None
        if binary_sha_before != binary_sha_after:
            errors.append("binary digest changed during run")
        if registry_sha is not None and registry_sha != sha256_file(REGISTRY):
            errors.append("registry digest changed during run")
        if runner_sha != sha256_file(RUNNER):
            errors.append("runner digest changed during run")
        for spec in specs:
            if sha256_file(HERE / spec["source"]) != spec["source_sha256"]:
                errors.append(f"source digest changed: {spec['source']}")
        if classes_path is not None:
            classes_sha_after = sha256_file(classes_path) if classes_path.is_file() else None
            if classes_sha_before != classes_sha_after:
                errors.append("expected-classes digest changed during run")
        if (stat.S_IMODE(out.stat().st_mode) != 0o700 or
                stat.S_IMODE((out / "logs").stat().st_mode) != 0o700):
            errors.append("output directory privacy mode changed during run")
        for entry in entries:
            for field in ("stdout", "stderr"):
                if stat.S_IMODE((out / entry[field]).stat().st_mode) != 0o600:
                    errors.append(f"transcript privacy mode changed: {entry[field]}")
    except OSError as error:
        errors.append(f"post-run identity check failed: {error.strerror}")

    expected = [(s["id"], s["form"]) for s in specs]
    actual = [(e["id"], e["form"]) for e in entries]
    if not specs or actual != expected:
        errors.append("results do not exactly cover ordered registry forms")
    results_file = out / "results.tsv"
    if results_file.exists():
        try:
            lines = results_file.read_text(encoding="utf-8").splitlines()
            persisted = [tuple(line.split("\t")[:2]) for line in lines[1:]]
            if (not lines or lines[0] != RESULT_HEADER or
                    persisted != actual or len(lines) != len(entries) + 1 or
                    any(len(line.split("\t")) != 13 for line in lines[1:])):
                errors.append("results.tsv does not exactly cover completed entries")
        except OSError as error:
            errors.append(f"results.tsv read failed: {error.strerror}")

    incomplete = any(e["result"].startswith("INCOMPLETE") or e["result"] == "INVALID"
                     for e in entries)
    failed = any(e["result"].startswith("FAIL") for e in entries)
    row_grade_status = ("incomplete" if errors or incomplete else
                        "has_failures" if failed else "provisional_all_matched")
    registry_bound = (args.expected_registry_sha256 is not None and
                      registry_sha == args.expected_registry_sha256.lower())
    classes_bound = (args.expected_classes_sha256 is not None and
                     classes_sha_before == args.expected_classes_sha256.lower() and
                     classes_sha_after == classes_sha_before)
    missing_classes = [f"{s['id']}/{s['form']}" for s in specs if s["intent"] != "ACCEPT"
                       and f"{s['id']}/{s['form']}" not in expectations]
    if not registry_bound and args.expected_registry_sha256 is None:
        errors.append("full-registry coverage is unbound: supply --expected-registry-sha256")
    if not classes_bound and args.expected_classes is None:
        errors.append("non-ACCEPT diagnostics are unbound: supply digest-bound --expected-classes")
    if missing_classes:
        errors.append("frozen non-ACCEPT expectations do not cover every registered form")
    if args.record is not None:
        errors.append("--record withheld: source commit has no verified build manifest or CAS history writer")
    errors.append("reviewed canonical matrix manifest is not integrated; aggregate completion unavailable")
    scope = ("provisional-operator-supplied" if registry_bound and classes_bound and
             not missing_classes else
             "class-unbound" if not classes_bound or missing_classes else
             "registry-unbound")
    status = "INCOMPLETE"
    counts = dict(sorted(collections.Counter(
        e["result"] for e in entries if e["form"] == "carrier").items()))
    all_counts = dict(sorted(collections.Counter(e["result"] for e in entries).items()))
    receipt = {
        "schema": "anubis-soundness-matrix-run/1", "phase": "final", "status": status,
        "row_grade_status": row_grade_status, "coverage_scope": scope,
        "canonical_manifest_integrated": False,
        "coverage_authority": "operator-supplied-digests",
        "non_claims": [
            "Operator-supplied digests do not independently establish the canonical registry.",
            "Rows graded against provisional expectations do not complete the full matrix.",
            "Obligation names and SMT digests are fingerprints, not stable independent IDs.",
            "A source-commit label is not bound to the measured compiler binary.",
        ],
        "binary_sha256_before": binary_sha_before,
        "binary_sha256_after": binary_sha_after,
        "binary_version_stdout": "binary-version.stdout.log" if version_stdout_sha else None,
        "binary_version_stdout_sha256": version_stdout_sha,
        "binary_version_stderr": "binary-version.stderr.log" if version_stderr_sha else None,
        "binary_version_stderr_sha256": version_stderr_sha,
        "binary_version_transcript_sha256": version_transcript_sha,
        "runner_sha256": runner_sha, "registry_sha256": registry_sha,
        "expected_registry_sha256": (args.expected_registry_sha256.lower()
                                     if args.expected_registry_sha256 else None),
        "expected_classes_sha256": (args.expected_classes_sha256.lower()
                                    if args.expected_classes_sha256 else None),
        "classes_sha256_before": classes_sha_before,
        "classes_sha256_after": classes_sha_after,
        "unbound_non_accept_forms": missing_classes,
        "timeout_seconds": args.timeout,
        "source_commit": args.record[1] if args.record else None,
        "source_commit_bound": False,
        "label": args.record[0] if args.record else None,
        "results": "results.tsv" if results_file.exists() else None,
        "results_sha256": sha256_file(results_file) if results_file.exists() else None,
        "expected_forms": len(expected), "completed_forms": len(entries),
        "primary_results": counts, "all_form_results": all_counts,
        "history_stage": None, "history_state": (
            "disabled-unbound-source-commit" if args.record is not None else "not_requested"),
        "history_appended": False, "errors": errors, "entries": entries,
    }
    # No tracked history mutation. A single atomic publication avoids a later
    # replacement failure invalidating an already published complete receipt.
    try:
        atomic_json(out / "receipt.json", receipt)
    except OSError as error:
        print(f"matrix error: final receipt publication failed: {error.strerror}",
              file=sys.stderr)
        return 2
    print(f"matrix status: {status}; primary results: {counts}; "
          f"completed forms: {len(entries)}/{len(expected)}", flush=True)
    for error in errors:
        print(f"matrix error: {error}", file=sys.stderr)
    return 2


signal.signal(signal.SIGTERM, interrupted)
sys.exit(main())
PY
