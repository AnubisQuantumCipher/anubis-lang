#!/usr/bin/env python3
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).parent / "lib" / "vz_apply_validate.py"
SPEC = importlib.util.spec_from_file_location("vz_apply_validate", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)


def valid_pair():
    def grant(capability, hypervisor_grant, tart_args=None, tart_enforced=False):
        return {
            "advisory": [], "capability": capability, "hypervisor_grant": hypervisor_grant,
            "needs_human": [], "present": False, "tart_args": tart_args or [],
            "tart_enforced": tart_enforced,
        }

    merkle = "a" * 64
    core = {
        "capabilities_present": [], "effects_bounded": True, "grants": [
            grant("net.send", "network:host-only", ["--net-host"], True),
            grant("fs.read", "mount:none"),
            grant("fs.write", "mount:none"),
            grant("shell", "no direct hypervisor grant"),
            grant("time.now", "no direct hypervisor grant"),
            grant("rand.gen", "no direct hypervisor grant"),
        ],
        "notes": [], "package": "demo", "research_effects": [],
        "schema": "anubis.confinement.v1", "source_merkle": merkle, "version": "0.0.0",
    }
    app = {
        "allow_hosts": [], "capabilities_present": [], "dns_pin_residual": "",
        "effects_bounded": True, "egress_pinned_ipv4": [], "mount_posture": "none",
        "mounts": [], "mounts_adjusted": [], "network_apply_mode": "host-only",
        "network_posture": "network:host-only", "network_tart_enforced": True,
        "notes": [], "program": "demo", "schema": "anubis.confinement.applied.v1",
        "source_merkle": merkle, "tart_args": ["--net-host"],
    }
    return core, app


class ValidatorTests(unittest.TestCase):
    def test_valid_pair(self):
        VALIDATOR.validate(*valid_pair())

    def assert_poison_rejected(self, mutate):
        core, app = valid_pair()
        mutate(core, app)
        with self.assertRaises(ValueError):
            VALIDATOR.validate(core, app)

    def test_extra_key(self):
        self.assert_poison_rejected(lambda core, app: core.__setitem__("extra", True))

    def test_missing_key(self):
        self.assert_poison_rejected(lambda core, app: app.pop("effects_bounded"))

    def test_null_security_field(self):
        self.assert_poison_rejected(lambda core, app: core.__setitem__("capabilities_present", None))

    def test_wrong_security_type(self):
        self.assert_poison_rejected(lambda core, app: app.__setitem__("effects_bounded", 1))

    def test_cross_artifact_drift(self):
        self.assert_poison_rejected(lambda core, app: app.__setitem__("source_merkle", "forged"))

    def test_version_must_be_string(self):
        self.assert_poison_rejected(lambda core, app: core.__setitem__("version", 1))

    def test_dns_pin_residual_must_be_string(self):
        self.assert_poison_rejected(lambda core, app: app.__setitem__("dns_pin_residual", None))

    def test_grant_boolean_must_be_boolean(self):
        self.assert_poison_rejected(lambda core, app: core["grants"][0].__setitem__("present", 0))

    def test_source_merkle_must_be_canonical(self):
        self.assert_poison_rejected(lambda core, app: core.__setitem__("source_merkle", "abc"))

    def test_grant_inventory_must_be_complete(self):
        self.assert_poison_rejected(lambda core, app: core.__setitem__("grants", []))

    def test_host_only_rejects_allow_hosts(self):
        self.assert_poison_rejected(lambda core, app: app.__setitem__("allow_hosts", ["example.com"]))

    def test_host_only_rejects_dns_pin_residual(self):
        self.assert_poison_rejected(
            lambda core, app: app.__setitem__("dns_pin_residual", "rebind_after_pin")
        )

    def test_cli_rejects_malformed_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            core_path = root / "core.json"
            app_path = root / "applied.json"
            core_path.write_text("{", encoding="utf-8")
            _, app = valid_pair()
            app_path.write_text(json.dumps(app), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), str(core_path), str(app_path)],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("VZ_APPLY_ARTIFACT_INVALID", result.stderr)


if __name__ == "__main__":
    unittest.main()
