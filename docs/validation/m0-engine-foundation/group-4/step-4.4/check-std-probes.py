#!/usr/bin/env python3
"""仅在私有临时目录运行两个独立安全反例；不修改仓库源码或用户文件。"""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent


def command(args):
    result = subprocess.run(args, capture_output=True, text=True)
    print(json.dumps({"command": [str(x) for x in args], "exit_code": result.returncode,
                      "stdout": result.stdout, "stderr": result.stderr}, ensure_ascii=False))
    result.check_returncode()
    return result.stdout


with tempfile.TemporaryDirectory(prefix="aline-44-std-probe-") as directory:
    directory = Path(directory)
    for name in ["std-permissions", "std-copy-path"]:
        source = ROOT / "fixtures" / f"{name}.rs"
        binary = directory / name
        command(["rustc", "--edition=2024", "-Dunsafe_code", str(source), "-o", str(binary)])
        case = directory / f"case-{name}"
        case.mkdir(mode=0o700)
        target = case / "target.md"
        target.write_bytes(b"OLD\n")
        target.chmod(0o600)
        if name == "std-permissions":
            command(["xattr", "-w", "com.aline.probe", "aline-preserve", str(target)])
            command(["chmod", "+a", "everyone allow read", str(target)])
            before = command(["ls", "-le", str(target)])
            print(json.dumps({"before_bytes": target.read_bytes().hex(),
                              "before_xattr": command(["xattr", "-p", "com.aline.probe", str(target)]).strip(),
                              "before_acl": before}, ensure_ascii=False))
            command([str(binary), str(case)])
            after = command(["ls", "-le", str(target)])
            print(json.dumps({"after_bytes": target.read_bytes().hex(), "after_acl": after,
                              "after_xattrs": command(["xattr", str(target)]).splitlines(),
                              "mode": oct(target.stat().st_mode & 0o777)}, ensure_ascii=False))
            assert target.read_bytes() == b"NEW\n"
            assert "com.aline.probe" not in command(["xattr", str(target)]).splitlines()
            assert "everyone allow read" not in after
        else:
            output = command([str(binary), str(case)])
            assert target.read_bytes() == b"", output
    print("confirmed=2 unsafe naive std-only approaches; no production primitive implemented")
