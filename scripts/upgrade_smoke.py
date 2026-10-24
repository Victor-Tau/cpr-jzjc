"""Run both release binaries with shared mock-host state to verify upgrade retention."""
import argparse
import copy
import json
import math
from pathlib import Path
import tarfile
import tempfile

from protocol_smoke import Host


def equivalent(before, after):
    # Stored JSON floats may round by one ULP when deserialized by serde_json.
    if isinstance(before, float):
        assert math.isclose(before, after, rel_tol=1e-14, abs_tol=0), (before, after)
    elif isinstance(before, dict):
        assert before.keys() == after.keys()
        for key in before:
            equivalent(before[key], after[key])
    elif isinstance(before, list):
        assert len(before) == len(after)
        for left, right in zip(before, after):
            equivalent(left, right)
    else:
        assert before == after, (before, after)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("previous_package", type=Path)
    parser.add_argument("current_package", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="modeltrace-upgrade-") as directory:
        roots = [Path(directory) / version for version in ["previous", "current"]]
        for package, root in zip([args.previous_package, args.current_package], roots):
            with tarfile.open(package) as archive:
                archive.extractall(root, filter="data")
        manifests = [json.loads((root / "plugin.json").read_text()) for root in roots]
        assert manifests[0]["state"] == manifests[1]["state"]
        old = Host(binary=roots[0] / "bin/modeltrace-health", protocol_version=manifests[0]["package"]["protocolVersion"])
        try:
            bootstrap = old.request("api/bootstrap")
            cfg = bootstrap["config"]
            cfg.update(keyId="key-test", lowConfidencePercent=0, degradedPercent=100,
                       confirmationRounds=4, lowConfidenceRetries=2, retryDelaySeconds=17,
                       retentionDays=47, timeoutSeconds=180)
            cfg["schedule"].update(enabled=True, scope="ids", accountIds=["acct-test"], timezone="Asia/Shanghai")
            old.request("api/config", {"config": cfg, "revision": bootstrap["configRevision"]})
            identity = old.request("api/submit", {"accountIds": ["acct-test"], "requestId": "actual-upgrade"})["submitted"][0]
            report = old.request("api/report?id=" + identity)
            # 1.4 defers scoring to the next maintenance callback when the callback
            # budget runs low (cold start), so one tick no longer settles a round.
            # Tick until the job settles: a first load after restart intentionally
            # interrupts active jobs, which would mask the retention comparison.
            for _ in range(5):
                if report["status"] not in ("queued", "running"):
                    break
                old.tick()
                report = old.request("api/report?id=" + identity)
            assert report["status"] not in ("queued", "running"), "job did not settle before upgrade"
            before = old.request("api/bootstrap")
            state = copy.deepcopy(old.state)
        finally:
            old.close()
        current = Host(binary=roots[1] / "bin/modeltrace-health", state=state, protocol_version=manifests[1]["package"]["protocolVersion"])
        try:
            after = current.request("api/bootstrap")
            restored = current.request("api/report?id=" + identity)
            replay = current.request("api/submit", {"accountIds": ["acct-test"], "requestId": "actual-upgrade"})
            assert replay["submitted"] == [identity], "upgrade must retain submission idempotency"
            for key, value in before["config"].items():
                if key == "retentionDays":
                    # 1.7.1 fixes history retention to the most recent three days.
                    assert value in (3,47) and after["config"][key] == 3, (value, after["config"][key])
                else:
                    equivalent(value, after["config"][key])
            for key in ["configRevision", "nextDue"]:
                assert after[key] == before[key], key
            assert not after["config"]["autoMoveGroup"]
            assert after["config"]["targetGroupId"] == ""
            assert restored["groupState"] == "none"
            for key in ["status", "verdict", "rounds", "attempts", "bankHash", "bankCommit", "disableState"]:
                previous = copy.deepcopy(report[key])
                if key == "attempts":
                    for attempt in previous:
                        # v1.4.1 adds optional timing diagnostics; old requests have none.
                        attempt.setdefault("diagnostics", None)
                        attempt.pop("usage", None)
                        attempt.pop("costCheckedAt", None)
                        if attempt['diagnostics'] is not None:
                            attempt['diagnostics'].setdefault('partial', False)
                equivalent(previous, restored[key])
            assert after["banks"]["active"]["hash"] == before["banks"]["active"]["hash"]
            assert len(current.executions) == 0
            print(f"Upgrade {manifests[0]['version']} -> {manifests[1]['version']}: settings, schedule, history and bank retained; no probe replay.")
        finally:
            current.close()


if __name__ == "__main__":
    main()
