# 嵌套 Cargo AST 审计

审计器解析 `scripts/*.py`，扫描命令执行器调用中 AST 可解析的 Cargo argv；仅序列化的 synthetic command 不算实际调用。

- 调用点：25
- 需要锁定检查：16
- 需要锁定但缺少 `--locked`：0
- 已锁定：16
- 初始生成例外：3
- 保留并说明理由的缺锁例外：0
- 语法解析失败：0

| 文件 | 行 | 子命令 | locked | offline | 初始生成例外 | 分类/处置 |
|---|---:|---|---:|---:|---:|---|
| `scripts/check_engineering.py` | 151 | `metadata` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_engineering.py` | 152 | `tree` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_engineering.py` | 155 | `metadata` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_lints.py` | 914 | `clippy` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_lints.py` | 917 | `clippy` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_lints.py` | 1227 | `check` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_lints.py` | 1238 | `check` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_lints.py` | 1201 | `check` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_lints.py` | 1222 | `clippy` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_lints.py` | 790 | `generate-lockfile` | 否 | 是 | 是 | 初始锁文件生成例外 |
| `scripts/check_lints.py` | 1175 | `-V` | 否 | 否 | 否 | 版本查询，不读取或改写 Cargo.lock |
| `scripts/check_lints.py` | 1177 | `clippy` | 否 | 否 | 否 | 版本查询，不读取或改写 Cargo.lock |
| `scripts/check_public_api.py` | 87 | `generate-lockfile` | 否 | 是 | 是 | 初始锁文件生成例外 |
| `scripts/check_public_api.py` | 91 | `check` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_safety.py` | 287 | `metadata` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_workspace.py` | 73 | `-V` | 否 | 否 | 否 | 版本查询，不读取或改写 Cargo.lock |
| `scripts/check_workspace.py` | 74 | `fmt` | 否 | 否 | 否 | 版本查询，不读取或改写 Cargo.lock |
| `scripts/check_workspace.py` | 75 | `clippy` | 否 | 否 | 否 | 版本查询，不读取或改写 Cargo.lock |
| `scripts/check_workspace.py` | 77 | `fmt` | 否 | 否 | 否 | 格式命令，不读取或改写依赖 Cargo.lock |
| `scripts/check_workspace.py` | 78 | `clippy` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/check_workspace.py` | 79 | `test` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/engineering_self_test.py` | 171 | `metadata` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/engineering_self_test.py` | 84 | `generate-lockfile` | 否 | 是 | 是 | 初始锁文件生成例外 |
| `scripts/engineering_self_test.py` | 88 | `metadata` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
| `scripts/engineering_self_test.py` | 90 | `tree` | 是 | 是 | 否 | 当前合规：locked-check 带 --locked |
