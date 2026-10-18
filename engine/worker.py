"""Bounded JSON-line worker. Only vendored code executes; downloaded banks are JSON data."""
from __future__ import annotations

from functools import lru_cache
import json
import math
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "vendor/modeltrace"))
import fingerprint as fp
import numpy as np

MAX_BANK = 20 * 1024 * 1024


def finite_array(value, shape, name):
    a = np.asarray(value, dtype=np.float64)
    if a.shape != shape or not np.isfinite(a).all():
        raise ValueError(f"指纹库矩阵无效：{name}")
    return a


@lru_cache(maxsize=1)
def builtin_bank():
    return json.loads((ROOT / "vendor/modeltrace/data/unified_bank.json").read_text())


def validate(bank):
    builtin = builtin_bank()
    if bank.get("schema") != builtin["schema"]:
        raise ValueError("指纹库 schema 不兼容，请升级插件")
    # Upstream has no numeric schema version; method metadata also forms the contract.
    for key in builtin["method"]:
        if builtin.get("method", {}).get(key) != bank.get("method", {}).get(key):
            raise ValueError("指纹方法已变化，请升级插件")
    models = bank.get("models", [])
    if not 2 <= len(models) <= 128:
        raise ValueError("指纹库须包含2至128个模型")
    ids = [m["id"] for m in models]
    if len(set(ids)) != len(ids) or any(not isinstance(i, str) or not 1 <= len(i) <= 128 for i in ids):
        raise ValueError("指纹库模型ID无效或重复")
    for m in models:
        if not isinstance(m.get("display_name"), str) or len(m["display_name"]) > 128:
            raise ValueError("模型名称无效")
        if not isinstance(m.get("family"), str) or len(m["family"]) > 64:
            raise ValueError("模型家族无效")
        counts = finite_array(m["counts"], (355,), "counts")
        if np.any(counts < 0) or counts.sum() <= 0:
            raise ValueError("指纹计数无效")
    robust = bank["robust"]
    if robust["model_order"] != ids:
        raise ValueError("指纹模型顺序不一致")
    for key, dimension in (("hellinger", 355), ("ordered_blocks", 74)):
        artifact = robust[key]
        finite_array(artifact["feature_mean"], (dimension,), key + ".mean")
        scale = finite_array(artifact["feature_scale"], (dimension,), key + ".scale")
        if np.any(scale <= 0):
            raise ValueError("指纹标准化尺度必须为正数")
        finite_array(artifact["centroids"], (len(ids), dimension), key + ".centroids")
        basis = artifact["nuisance_basis"]
        if basis:
            finite_array(basis, (len(basis), dimension), key + ".basis")
        if key == "ordered_blocks":
            templates = artifact["environment_centroids"]
            if not 1 <= len(templates) <= 64:
                raise ValueError("指纹环境数量无效")
            finite_array(templates, (len(templates), len(ids), dimension), key + ".environments")
            if artifact.get("weight") != builtin["robust"][key]["weight"]:
                raise ValueError("指纹融合权重已变化，请升级插件")
    for n in ("1", "2", "3"):
        c = bank["calibration"][n]
        if not math.isfinite(c["beta"]) or not 0 < c["beta"] <= 10000:
            raise ValueError("指纹校准参数无效")
        if not math.isfinite(c["cv_accuracy"]) or not 0 <= c["cv_accuracy"] <= 1:
            raise ValueError("指纹校准准确率无效")
    # Exercise every calibration path without sending a model request.
    output = {"text": ",".join(str((i * 97 + i * i) % 355 + 1) for i in range(310)), "expected_count": 310}
    for n in (1, 2, 3):
        result = fp.analyze_global_outputs([output] * n, bank)
        probabilities = [r["probability"] for r in result["results"]]
        if not all(math.isfinite(p) and 0 <= p <= 1 for p in probabilities) or abs(sum(probabilities) - 1) > 1e-8:
            raise ValueError("指纹库离线评分自检失败")
    return {"builtAt": bank.get("built_at", ""), "models": [
        {"id": m["id"], "label": m["display_name"], "family": m["family"]} for m in models]}


def load(path):
    p = Path(path)
    stat = p.stat()
    if stat.st_size > MAX_BANK:
        raise ValueError("指纹库超过20MiB")
    return cached_load(str(p), stat.st_mtime_ns, stat.st_size)


@lru_cache(maxsize=4)
def cached_load(path, modified, size):
    return json.loads(Path(path).read_text(), parse_constant=lambda x: (_ for _ in ()).throw(ValueError("非有限JSON数值")))


def handle(data):
    operation = data["op"]
    if operation == "challenges":
        return fp.generate_challenges(1)
    if operation == "validate":
        return validate(load(data["path"]))
    if operation == "sample":
        text = data["text"]
        if len(text.encode()) > 1024 * 1024:
            raise ValueError("回答过大")
        numbers = fp.parse_numbers(text)
        minimum = max(80, math.ceil(data["expectedCount"] * 0.55))
        if not minimum <= len(numbers) <= 4096:
            raise ValueError(f"有效数字不足或过多（{len(numbers)}，至少{minimum}，至多4096）")
        return numbers
    if operation == "score":
        outputs = [{"text": ",".join(map(str, s["numbers"])), "expected_count": s["expectedCount"]} for s in data["samples"]]
        if len(outputs) != 1:
            raise ValueError("每轮评分需要一个有效样本")
        r = fp.analyze_global_outputs(outputs, load(data["path"]))
        return {"prediction": r["prediction"], "probability": r["probability"],
                "results": [{"model": x["model"], "label": x["display_name"], "family": x["family"], "probability": x["probability"]} for x in r["results"]],
                "families": r["family_probabilities"]}
    raise ValueError("未知评分操作")


def main():
    for line in sys.stdin.buffer:
        try:
            if len(line) > 2 * 1024 * 1024:
                raise ValueError("评分输入过大")
            result = {"ok": True, "data": handle(json.loads(line))}
        except Exception as error:
            result = {"ok": False, "error": str(error)[:400]}
        print(json.dumps(result, ensure_ascii=False, allow_nan=False, separators=(",", ":")), flush=True)


if __name__ == "__main__":
    main()
