"""Compare round callback counts and local execution time using a mock model.

This does not measure real model latency. Host scheduling gaps are calculated
separately at the RS 3.29.1 maintenance interval of 30 seconds.
"""
import argparse
import json
from pathlib import Path
import time

from protocol_smoke import Host


def measure(binary):
    host = Host(binary=binary.resolve())
    try:
        bootstrap = host.request("api/bootstrap")
        config = bootstrap["config"]
        config.update(keyId="key-test", lowConfidencePercent=0, degradedPercent=100, retryDelaySeconds=0)
        host.request("api/config", {"config":config, "revision":bootstrap["configRevision"]})
        results = []
        for kind in ("cold", "warm"):
            identity = host.request("api/submit", {"accountIds":["acct-test"], "requestId":"benchmark-"+kind})["submitted"][0]
            elapsed = 0
            for callbacks in range(1, 9):
                started = time.monotonic()
                host.tick()
                elapsed += time.monotonic() - started
                report = host.request("api/report?id="+identity)
                if report["status"] == "completed":
                    break
                assert report["status"] in ("queued", "running"), report["message"]
            assert report["status"] == "completed" and len(report["rounds"]) == 1
            results.append({"kind":kind, "callbacks":callbacks, "localSeconds":round(elapsed, 4), "schedulingGapSeconds":(callbacks-1)*30})
        return results
    finally:
        host.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("current", type=Path)
    args = parser.parse_args()
    print(json.dumps({"baseline":measure(args.baseline), "current":measure(args.current)}, indent=2))
