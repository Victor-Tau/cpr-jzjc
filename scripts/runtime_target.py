"""Pinned inputs and ELF checks shared by runtime build, reuse and verification."""

TARGETS = {
    "x86_64-unknown-linux-gnu": {
        "image": "index.docker.io/library/python@sha256:8a00ecb83ed9d0864dfd102a1bdb2067fda907ceb30bb86d09da9ee558b19590",
        "docker_platform": "linux/amd64",
        "architecture": "amd64",
        "libdir": "x86_64-linux-gnu",
        "loader": "ld-linux-x86-64.so.2",
        "wheel_platforms": ("manylinux_2_27_x86_64", "manylinux_2_28_x86_64"),
        "config": "sha256:141fe9b385f3bb6d618c07ace1836c0b3c72bf525566ac3eb13cd1f405069877",
        "pin_file": "scripts/numpy-wheel.sha256",
        "elf_machine": 62,
    },
    "aarch64-unknown-linux-gnu": {
        "image": "index.docker.io/library/python@sha256:8f51ccb8569b75cf967ffe3e1638b9953042bf36142d7032cbb6f0696c9cd2c6",
        "docker_platform": "linux/arm64",
        "architecture": "arm64",
        "libdir": "aarch64-linux-gnu",
        "loader": "ld-linux-aarch64.so.1",
        "wheel_platforms": ("manylinux_2_27_aarch64", "manylinux_2_28_aarch64"),
        "config": "sha256:210294d6efde2595a5c75a057f33168eca775f5bdcba0ce761cd17593e1724d7",
        "pin_file": "scripts/numpy-wheel-aarch64.sha256",
        "elf_machine": 183,
    },
}


def validate_elf(header, target):
    if len(header) < 20 or header[:6] != b"\x7fELF\x02\x01":
        raise ValueError("Expected a 64-bit little-endian ELF file")
    if int.from_bytes(header[18:20], "little") != TARGETS[target]["elf_machine"]:
        raise ValueError(f"ELF architecture does not match {target}")
