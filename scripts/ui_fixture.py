"""Generate browser fixtures using the real plugin and the deterministic RPC host."""
import argparse
import copy
import json
from pathlib import Path

from protocol_smoke import BINARY, ROOT, Host


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=BINARY)
    parser.add_argument("--output", type=Path, default=ROOT / ".cache/ui-fixture.json")
    args = parser.parse_args()
    host = Host(binary=args.binary)
    try:
        initial = host.request("api/bootstrap")
        display_config = copy.deepcopy(initial["config"])
        display_config["keyId"] = "key-test"
        sample_config = copy.deepcopy(display_config)
        sample_config.update(lowConfidencePercent=0, degradedPercent=100, retryDelaySeconds=0)
        host.request("api/config", {"config": sample_config, "revision": initial["configRevision"]})
        job = host.request("api/submit", {"accountIds": ["acct-test"], "requestId": "ui-fixture"})["submitted"][0]
        host.tick()
        bootstrap = host.request("api/bootstrap")
        bootstrap["config"] = display_config
        accounts = []
        for i in range(43):
            accounts.append({
                "id": "acct-test" if i == 1 else f"acct-{i}",
                "name": f"测试账户 {i}",
                "email": "user@example.com" if i == 1 else f"account-{i}@example.com",
                "provider": "openai", "supported": True,
                "enabled": i % 7 != 0,
                "status": "error" if i % 10 == 0 else "quota_exhausted" if i % 11 == 0 else "rate_limited" if i % 13 == 0 else "normal",
                "planType": ["free", "plus", "pro", "team"][i % 4],
                "groups": [{"id": f"group-{i % 3}", "name": ["默认组", "业务组", "备用组"][i % 3]}],
            })
        fixture = {"bootstrap": bootstrap, "accounts": accounts, "report": host.request("api/report?id=" + job)}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(fixture, ensure_ascii=False, indent=2), encoding="utf-8")
        print(args.output)
    finally:
        host.close()


if __name__ == "__main__":
    main()
