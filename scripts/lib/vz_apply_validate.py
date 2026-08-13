#!/usr/bin/env python3
"""Strict validator for VZ confinement core/applied artifacts."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

EXPECTED_CORE_KEYS = {
    "capabilities_present", "effects_bounded", "grants", "notes", "package",
    "research_effects", "schema", "source_merkle", "version",
}
EXPECTED_APPLIED_KEYS = {
    "allow_hosts", "capabilities_present", "dns_pin_residual", "effects_bounded",
    "egress_pinned_ipv4", "mount_posture", "mounts", "mounts_adjusted",
    "network_apply_mode", "network_posture", "network_tart_enforced", "notes",
    "program", "schema", "source_merkle", "tart_args",
}
EXPECTED_GRANT_KEYS = {
    "advisory", "capability", "hypervisor_grant", "needs_human",
    "present", "tart_args", "tart_enforced",
}
EXPECTED_GRANT_CAPABILITIES = {
    "fs.read", "fs.write", "net.send", "rand.gen", "shell", "time.now",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def is_str_list(value: Any) -> bool:
    return isinstance(value, list) and all(isinstance(item, str) for item in value)


def validate(core: Any, app: Any) -> None:
    require(isinstance(core, dict), "core artifact is not an object")
    require(isinstance(app, dict), "applied artifact is not an object")
    require(
        set(core) == EXPECTED_CORE_KEYS,
        f"core keyset missing={sorted(EXPECTED_CORE_KEYS - set(core))!r} "
        f"extra={sorted(set(core) - EXPECTED_CORE_KEYS)!r}",
    )
    require(
        set(app) == EXPECTED_APPLIED_KEYS,
        f"applied keyset missing={sorted(EXPECTED_APPLIED_KEYS - set(app))!r} "
        f"extra={sorted(set(app) - EXPECTED_APPLIED_KEYS)!r}",
    )
    require(core.get("schema") == "anubis.confinement.v1", f"core schema={core.get('schema')!r}")
    require(
        app.get("schema") == "anubis.confinement.applied.v1",
        f"applied schema={app.get('schema')!r}",
    )
    require(isinstance(core.get("source_merkle"), str) and bool(core["source_merkle"]), "core source_merkle is empty or not string")
    require(
        re.fullmatch(r"[0-9a-f]{64}", core["source_merkle"]) is not None,
        "core source_merkle is not canonical lowercase sha256",
    )
    require(isinstance(core.get("package"), str) and bool(core["package"]), "core package missing or not string")
    require(isinstance(core.get("version"), str) and bool(core["version"]), "core version missing or not string")
    require(
        isinstance(core.get("grants"), list)
        and all(isinstance(item, dict) for item in core["grants"]),
        "core grants missing or not object list",
    )
    for index, grant in enumerate(core["grants"]):
        require(set(grant) == EXPECTED_GRANT_KEYS, f"grant[{index}] keyset drift")
        require(is_str_list(grant.get("advisory")), f"grant[{index}].advisory not string list")
        require(
            isinstance(grant.get("capability"), str) and bool(grant["capability"]),
            f"grant[{index}].capability missing or not string",
        )
        require(
            isinstance(grant.get("hypervisor_grant"), str) and bool(grant["hypervisor_grant"]),
            f"grant[{index}].hypervisor_grant missing or not string",
        )
        require(is_str_list(grant.get("needs_human")), f"grant[{index}].needs_human not string list")
        require(isinstance(grant.get("present"), bool), f"grant[{index}].present not bool")
        require(is_str_list(grant.get("tart_args")), f"grant[{index}].tart_args not string list")
        require(isinstance(grant.get("tart_enforced"), bool), f"grant[{index}].tart_enforced not bool")
    grant_capabilities = [grant["capability"] for grant in core["grants"]]
    require(
        len(grant_capabilities) == len(set(grant_capabilities)),
        "grant capabilities are not unique",
    )
    require(
        set(grant_capabilities) == EXPECTED_GRANT_CAPABILITIES,
        "grant capability inventory drift",
    )
    require(is_str_list(core.get("notes")), "core notes missing or not string list")
    require(is_str_list(core.get("research_effects")), "core research_effects missing or not string list")
    require(app.get("source_merkle") == core["source_merkle"], "applied source_merkle does not match core")
    require(isinstance(core.get("effects_bounded"), bool), "core effects_bounded missing or not bool")
    require(isinstance(app.get("effects_bounded"), bool), "applied effects_bounded missing or not bool")
    require(app["effects_bounded"] == core["effects_bounded"], "effects_bounded drift")
    require(isinstance(core.get("capabilities_present"), list), "core capabilities_present missing or not list")
    require(isinstance(app.get("capabilities_present"), list), "applied capabilities_present missing or not list")
    require(is_str_list(core["capabilities_present"]), "core capabilities_present is not string list")
    require(is_str_list(app["capabilities_present"]), "applied capabilities_present is not string list")
    require(app["capabilities_present"] == core["capabilities_present"], "capabilities_present drift")
    require(app.get("tart_args") == ["--net-host"], f"unexpected tart_args={app.get('tart_args')!r}")
    require(is_str_list(app.get("allow_hosts")), "allow_hosts missing or not string list")
    require(is_str_list(app.get("egress_pinned_ipv4")), "egress_pinned_ipv4 missing or not string list")
    require(isinstance(app.get("dns_pin_residual"), str), "dns_pin_residual missing or not string")
    require(is_str_list(app.get("notes")), "applied notes missing or not string list")
    require(isinstance(app.get("program"), str) and bool(app["program"]), "program missing or not string")
    require(app.get("mount_posture") == "none", f"mount_posture={app.get('mount_posture')!r}")
    require(app.get("mounts") == [], f"mounts={app.get('mounts')!r}")
    require(app.get("mounts_adjusted") == [], f"mounts_adjusted={app.get('mounts_adjusted')!r}")
    require(app.get("network_posture") == "network:host-only", f"network_posture={app.get('network_posture')!r}")
    require(app.get("network_apply_mode") == "host-only", f"network_apply_mode={app.get('network_apply_mode')!r}")
    require(app.get("network_tart_enforced") is True, f"network_tart_enforced={app.get('network_tart_enforced')!r}")
    require(app["allow_hosts"] == [], "host-only artifact has allow_hosts")
    require(app["egress_pinned_ipv4"] == [], "host-only artifact has pinned egress")
    require(app["dns_pin_residual"] == "", "host-only artifact has DNS pin residual")


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(f"usage: {argv[0]} CORE_JSON APPLIED_JSON", file=sys.stderr)
        return 2
    try:
        core = json.loads(Path(argv[1]).read_text(encoding="utf-8"))
        app = json.loads(Path(argv[2]).read_text(encoding="utf-8"))
        validate(core, app)
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as exc:
        print(f"VZ_APPLY_ARTIFACT_INVALID: {exc}", file=sys.stderr)
        return 1
    print("applied_ok", app["tart_args"], "mount_posture", app["mount_posture"], "net_mode", app["network_apply_mode"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
