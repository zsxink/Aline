# M4 后端与平台适配准入审查

状态：**任务 1.5 完成（2026-11-26 用户裁决）。** 本记录冻结了 1.2～1.4 支持契约对应的候选后端、版本、校验和、许可证、依赖边、目标/features 与逐场景门禁裁决；三项待裁决已全部批准（见“裁决结果”），禁库守卫与五个运行时准入分组已按裁决登记并入正式策略，`theme-zip` 分组与七场景 GPUI 指纹按接线顺序在对应任务登记。探针在正式策略上复跑 `PROBE_OK groups=11 failed=0`。

已可准入（零新增包、零漂移）：`usvg@0.46.0`、`resvg@0.46.0`、`tiny-skia@0.11.4` 三个直依赖，以及 M4 全部 8 项平台适配（锁定 GPUI 既有 API，零新增依赖）。“依赖已明确”不等于对应任务已完成；生产接线仍按任务顺序在 3.x/4.x/5.x 进行。

## 方法与可重复性

- 规则层 `scripts/m4_admission_rules.py`：纯常量与纯函数，冻结候选身份、直接依赖行、分组期望、内嵌素材、平台适配与策略快照 SHA-256；不读磁盘、不起进程。
- 探针 `scripts/probe_m4_admission.py`：把八个 workspace 成员、`vendor/` 与四份根清单复制到 `<tmp>` 工作区副本，按分组注入候选直接依赖，逐场景执行 `cargo metadata`/`cargo tree`，再用**门禁自己的** `engineering_dependencies.validate()` 裁决；同时用门禁自己的 `gpui_snapshot()`/`fingerprint()` 重算 GPUI 受审子图指纹，使被指纹检查拒绝的组也能给出拟批准值。
- 复跑命令：`python3 -I scripts/probe_m4_admission.py --self-test` 与 `python3 -I scripts/probe_m4_admission.py --offline`。裁决前测量：`SELFTEST_OK groups=10 packages=29 scenarios=7`、`PROBE_OK groups=10 failed=0`；裁决并登记策略后：`SELFTEST_OK groups=11 packages=29 scenarios=7`、`PROBE_OK groups=11 failed=0`。新增的第 11 组 `production` 是 M4 终态组合（全部已批准后端 + SVG 直依赖）。
- 环境：macOS 27.2.1 arm64、cargo/rustc 1.98.1、Python 3.9.6，cargo 强制离线并使用本机 registry 缓存。原始 metadata/tree/lock 与 `summary.json`/`report.md` 位于 `<repo>/logs/validation/m4-plugins-and-themes/2026-11-26-admission/review-probe/`（ignored），不是仓库公共报告。
- **口径声明**：`m4_admission_rules.py` 的期望值来自本轮同一次测量的冻结，探针再跑一遍只证明**可复现性与门禁当前行为**，不构成对候选的独立批准；独立批准只能来自用户裁决。
- 策略快照：`scripts/engineering_policy.json` 与 `scripts/dependency_admission_policy.json` 的 SHA-256 冻结在规则层；策略一旦变化，探针先失败而不是静默沿用旧期望。

### 为什么直接调 `validate()` 而不走 `check_engineering.py --output-dir`

该入口把 argv 与路径归一化成 `<tmp>`，而本地来源身份只允许还原 `<repo>`，工作区副本必然报“本地来源身份/路径漂移”。那是入口缺陷，不是依赖结论。探针以原始 metadata/tree/lock 调 `validate()`，覆盖要回答的四层（图身份、原生目标路径、GPUI 指纹、运行时准入分组），拒绝顺序与正式门禁一致：结构 → tree 可达 → 原生路径 → metadata 旁路 → 禁库入边 → GPUI 指纹 → 准入分组。

### 两个已修正的测量陷阱

- `engineering_dependencies.reachable(omit=…)` 的 `omit` 只接受**单个身份**；传集合时阻断失效，会把基线里的 wasi/wasip2 全报成旁路。原生路径一律交给 `check_native_paths()` / `engineering_native_paths._walk()`（`blocked` 是集合）。
- `engineering_dependencies.route()` 求的是全图最短路，会穿过 GPUI；用它描述旁路会把 `docx-rs→…→wasip2` 误写成 `gpui-pre→getrandom→wasip2`。探针改用绕开原生后端根与 core 的 BFS（`route_avoiding`）。

## 1.2～1.4 支持契约 → 依赖对照

| 契约来源 | 支持契约 | 依赖（精确身份与 features） | 依赖层面状态 |
|---|---|---|---|
| V-02 EXPORT-02～04 | PDF 纸型/方向/四边页边距、CJK 子集、产物可用 | `krilla@0.8.2`，`default-features=false`，`raster-images`+`simple-text` | 待裁决（GPUI 漂移 + 新准入分组） |
| V-02 EXPORT-02～04 | Word 标题/正文/代码样式、可编辑边界 | `docx-rs@0.4.22`，`default-features=false`（**不得启用 `image`**） | 待裁决（同上；`image` 会触发原生路径拒绝） |
| V-04/V-06 | 主题包/资源 ZIP 的条目、解压与容量预算 | `zip@8.6.0`，`default-features=false`，`deflate-flate2-zlib-rs` | 待裁决（flate2 漂移） |
| V-03 SVG 安全边界 | 只允许 `data:` 的 `ImageHrefResolver` | `usvg@0.46.0`、`resvg@0.46.0`、`tiny-skia@0.11.4` 直依赖 | **可准入**：零新增包、零漂移 |
| V-03 | 23 类 Mermaid 图类型，只取 SVG | `mermaid-rs-renderer@0.3.1`，`default-features=false`（不启用 `png`/`cli`） | 待裁决（禁库守卫） |
| PLUGIN-05 / PL-02 | 语言识别、家族/模式、未知语言回退 | `syntect@5.3.0`，`parsing`+`default-syntaxes`+`default-themes`+`dump-load`+`regex-fancy` | 待裁决（禁库守卫） |
| V-03 | 7 类公式语法 | `latex-rust@2.1.1`，`default-features=false`，`std` | 待裁决（内嵌素材许可证 + 新准入分组） |
| 任务 3.4/4.4/5.2/5.4 | 打开链接、揭示/打开文件、选路径、窗口外观 | 锁定 GPUI 既有 API（见下节） | **可准入**：零新增依赖 |

### 契约缺口与更正（只追加，不倒改历史记录）

1. **4.1 的“1.3/1.4 冻结的语言集”在现有文档中并不存在。** `tasks.md` 1.3 冻结的是图片格式/动画、SVG 边界、剪贴板、链接 allowlist、7 类公式、23 类 Mermaid 与主题导入校验；1.4 冻结的是预算。没有任何工件冻结高亮语言的支持/拒绝清单。该缺口决定 `syntect` 是随 `default-syntaxes` 全量发布，还是改为受控的子集 dump（需在构建期生成二进制资源），因此 4.1 的接线前置条件不完整，列入待确认；它不改变本轮依赖身份测量。
2. **更正 V-03 的 `ttf-parser` 陈述。** `docs/v1-validation-plan.md` V-03 记“`latex-rust` 带入 RUSTSEC-2026-0192，ttf-parser 0.25.1 unmaintained”。实测：`ttf-parser@0.25.1` 已在基线闭包内（锁定 GPUI 的 usvg/fontdb 路径），`latex-rust@2.1.1` 关闭默认 features 后**只新增它自己一个包**，不引入 `ttf-parser`，该公告属于既有依赖而非本次引入。此更正不缩短原公告的适用性，只纠正归属。
3. **更正 V-03 的 Mermaid 闭包陈述。** V-03 记“mermaid 带入 resvg/usvg 0.47”。在 `default-features=false` 下实测只新增 `mermaid-rs-renderer@0.3.1`、`json5@1.3.8`、`ucd-trie@0.1.8` 三个包，不带 usvg/resvg 0.47；`png` feature 才会带入，且本轮明确不启用。
4. V-03 探针独立解析到 `regex@1.13.2`，本工作区为 `regex@1.13.1`。本轮工作区副本中 `mermaid-rs-renderer → regex@1.13.1`，与已登记精确例外同版本；版本差异是隔离探针环境事实，不等于工作区新增禁库。

## 候选直接依赖

| 身份 | 校验和 | 许可证 | rust-version | features | 消费方 | 状态 |
|---|---|---|---|---|---|---|
| `zip@8.6.0` | `2d04a6b5381502aa6087c94c669499eb1602eb9c5e8198e534de571f7154809b` | MIT | 1.88 | `deflate-flate2-zlib-rs` | aline-plugin | 已批准（2026-11-26） |
| `krilla@0.8.2` | `27da593198b20eeba65caeb73c2bbeec3e53ab08fa549898312ce81c4fce5e33` | MIT OR Apache-2.0 | 1.92 | `raster-images, simple-text` | aline-plugin | 已批准（2026-11-26） |
| `docx-rs@0.4.22` | `7fdf00e8af6d0b3e92d4bbf9b76f773d8b84ea80f310324ad16cbdc2e653e02c` | MIT | — | 无 | aline-plugin | 已批准（2026-11-26） |
| `latex-rust@2.1.1` | `f8094b939c8af51f9c7d08256ffa607b743e24d466d9a0ccc602dc748c911e54` | MIT OR Apache-2.0 | 1.76 | `std` | aline-plugin | 已批准，随附许可证（2026-11-26） |
| `mermaid-rs-renderer@0.3.1` | `6cb5b469222ce6b9896edf0a969cf68464f5a028ef394fd6643f00bd943df117` | MIT | — | 无 | aline-plugin | 已批准并登记入边（2026-11-26） |
| `syntect@5.3.0` | `656b45c05d95a5704399aeef6bd0ddec7b2b3531b7c9e900abbf7c4d2190c925` | MIT | — | `parsing, default-syntaxes, default-themes, dump-load, regex-fancy` | aline-plugin | 已批准并登记入边（2026-11-26） |
| `usvg@0.46.0` | 基线已有 | Apache-2.0 OR MIT | — | 无 | aline-ui | 可准入 |
| `resvg@0.46.0` | 基线已有 | Apache-2.0 OR MIT | — | 无 | aline-ui | 可准入 |
| `tiny-skia@0.11.4` | 基线已有 | BSD-3-Clause | — | `std, simd` | aline-ui | 可准入 |

`zip` 必须显式选 `deflate-flate2-zlib-rs`：只写 `deflate-flate2` 时 Cargo 拒绝编译（“No compression backend selected”）。`deflate` 会额外启用 `zopfli` 写端；1.4 的预算以 `deflate` 测得，读路径同为 flate2/zlib-rs，因此生产读路径不受影响。

## 逐组实测裁决（七场景）

| 分组 | metadata 包 | 相对基线新增包 | 七场景裁决 | 期望一致 |
|---|---|---|---|---|
| baseline | 435 | 0 | pass | 是 |
| svg-direct | 435 | 0 | pass | 是 |
| zip | 437 | 2 | gpui-drift | 是 |
| krilla | 445 | 10 | gpui-drift | 是 |
| docx-image（`image` feature） | 444 | 9 | native-path | 是 |
| docx-plain | 444 | 9 | gpui-drift | 是 |
| syntect | 441 | 6 | guard-bypass | 是 |
| mermaid | 438 | 3 | guard-bypass | 是 |
| latex | 436 | 1 | admission-group | 是 |
| full-plain | 455 | 20 | gpui-drift | 是 |
| production（终态组合） | 464 | 29 | gpui-drift | 是 |

- 基线在七个场景全部通过，指纹与当前已批准的七场景值逐一相同；选包数为 all/all-features 394、macos-arm 332、macos-intel 333、windows-x64 286、linux-x64 278、linux-arm 277。
- `svg-direct` 在七个场景全部通过且指纹不变：三个直依赖已在锁定 GPUI 闭包内，仅增加调用点。
- 裁决并登记后的第二次实测：`baseline`、`svg-direct`、`latex` 七场景 `pass`；其余组因 GPUI 指纹仍是 M1 值而报 `gpui-drift`；`docx-image` 仍在原生路径层被拒。所有组的期望与实际一致。
- 除 `latex` 外，各组拒绝消息分类稳定，未出现“期望类别不符”或无法归类的消息。

## 新增包身份（29 个）

许可证明细均可被 `dependency_admission.license_review()` 判为允许；下表只列每组的完整身份，重复出现表示该组各自测到。

| 分组 | 包 | 校验和 | 许可证 | rust-version |
|---|---|---|---|---|
| zip | `typed-path@0.12.4` | `54ec2755506471f54baf5e4bb5499d8e510cc1caf271270e778a736bd2d7336c` | MIT OR Apache-2.0 | 1.65.0 |
| zip | `zip@8.6.0` | `2d04a6b5381502aa6087c94c669499eb1602eb9c5e8198e534de571f7154809b` | MIT | 1.88 |
| krilla | `bytemuck_derive@1.12.1` | `6a1f896587b6f2c069c73d2f0913e2d590c3990285cd2f0b6aa02b786b4c679c` | Zlib OR Apache-2.0 OR MIT | 1.71 |
| krilla | `font-types@0.11.3` | `5b38ad915f6dadd993ced50848a8291a543bd41ca62bc10740d5e64e2ab4cfd7` | MIT OR Apache-2.0 | 1.85 |
| krilla | `krilla@0.8.2` | `27da593198b20eeba65caeb73c2bbeec3e53ab08fa549898312ce81c4fce5e33` | MIT OR Apache-2.0 | 1.92 |
| krilla | `pdf-writer@0.15.0` | `f5e456864a7a304047bff84977dc6fb162bd956475d40ba50b2dcecaada7f753` | MIT OR Apache-2.0 | — |
| krilla | `read-fonts@0.39.2` | `c4ed38b89c2c77ff968c524145ad65fb010f38af5c7a224b53b81d47ac2daa81` | MIT OR Apache-2.0 | 1.85 |
| krilla | `skrifa@0.42.1` | `0c34617370ae968efb7161bb2beb517d9084659aae19e24b89e3db25b46e4564` | MIT OR Apache-2.0 | 1.85 |
| krilla | `subsetter@0.2.6` | `38803281d1c23166c5ebcb455439a5d2afe711cc909cf88af72448c297756ad6` | MIT OR Apache-2.0 | 1.85 |
| krilla | `tiny-skia-path@0.12.0` | `edca365c3faccca67d06593c5980fa6c57687de727a03131735bb85f01fdeeb9` | BSD-3-Clause | — |
| krilla | `write-fonts@0.48.1` | `cb731d4c4d93eacc69a1ad2f270f905788a98e4a3438267bcafbe08d3431c8d8` | MIT OR Apache-2.0 | 1.85 |
| krilla | `xmp-writer@0.3.3` | `9440ea3e5aeabb0ac63af70daf835274065238cdd0cec83418f417eae38bacee` | MIT OR Apache-2.0 | — |
| docx-image / docx-plain | `core_detect@1.0.0` | `7f8f80099a98041a3d1622845c271458a2d73e688351bf3cb999266764b81d48` | MIT/Apache-2.0 | — |
| docx-image / docx-plain | `docx-rs@0.4.22` | `7fdf00e8af6d0b3e92d4bbf9b76f773d8b84ea80f310324ad16cbdc2e653e02c` | MIT | — |
| docx-image / docx-plain | `encoding_rs@0.8.44` | `89253175e704ab869b04f9db224d38630e0211a53841a79181deae0889e169f5` | (Apache-2.0 OR MIT) AND BSD-3-Clause | 1.88 |
| docx-image / docx-plain | `multiversion_no_op@1.0.0` | `743fb55ba31b18fb1ecef6bdc9aa2743314978ac084044301a7eee33fb99a20d` | Apache-2.0 OR MIT | 1.56 |
| docx-image / docx-plain | `quick-xml@0.41.0` | `e660451e55124f798a69a5af3f49ccfbefbd41910eefd25caf2393e1f3473ec1` | MIT | 1.79 |
| docx-image / docx-plain | `simdutf8@0.1.5` | `e3a9fe34e3e7a50316060351f37187a3f546bce95496156754b601a5fa71b76e` | MIT OR Apache-2.0 | — |
| docx-image / docx-plain | `typed-path@0.12.4` | `54ec2755506471f54baf5e4bb5499d8e510cc1caf271270e778a736bd2d7336c` | MIT OR Apache-2.0 | 1.65.0 |
| docx-image / docx-plain | `zip@8.6.0` | `2d04a6b5381502aa6087c94c669499eb1602eb9c5e8198e534de571f7154809b` | MIT | 1.88 |
| docx-image / docx-plain | `zopfli@0.8.4` | `aaf7fc5d30c28483d93805c4a5e12b05bbb52407fa67c5f8bd552374cd01fb11` | Apache-2.0 | 1.88.0 |
| syntect | `bincode@1.3.3` | `b1f45e9417d87227c7a56d22e471c6206462cba514c7590c09aff4cf6d1ddcad` | MIT | — |
| syntect | `bit-set@0.8.0` | `08807e080ed7f9d5433fa9b275196cfc35414f66a0c79d864dc51a0d825231a3` | Apache-2.0 OR MIT | — |
| syntect | `bit-vec@0.8.0` | `5e764a1d40d510daf35e07be9eb06e75770908c27d411ee6c92109c9840eaaf7` | Apache-2.0 OR MIT | — |
| syntect | `fancy-regex@0.16.2` | `998b056554fbe42e03ae0e152895cd1a7e1002aec800fdc6635d20270260c46f` | MIT | 1.66 |
| syntect | `fnv@1.0.7` | `3f9eec918d3f24069decb9af1554cad7c880e2da24a9afd88aca000531ab82c1` | Apache-2.0 / MIT | — |
| syntect | `syntect@5.3.0` | `656b45c05d95a5704399aeef6bd0ddec7b2b3531b7c9e900abbf7c4d2190c925` | MIT | — |
| mermaid | `json5@1.3.8` | `6c1d229cedc4e210dd2e2a6cdc59e36ce28e7338fb69367d0d2566e26de7d599` | MIT | — |
| mermaid | `mermaid-rs-renderer@0.3.1` | `6cb5b469222ce6b9896edf0a969cf68464f5a028ef394fd6643f00bd943df117` | MIT | — |
| mermaid | `ucd-trie@0.1.8` | `8f20bdbfc9915686faa18fb5bbe8b4552778505defbf9f12289beefbc44092a6` | MIT OR Apache-2.0 | — |
| latex | `latex-rust@2.1.1` | `f8094b939c8af51f9c7d08256ffa607b743e24d466d9a0ccc602dc748c911e54` | MIT OR Apache-2.0 | 1.76 |

`full-plain` 的新增包是 krilla、docx-plain、latex 三组的并集（20 个）；`production` 的新增包是 krilla、docx-plain、latex、syntect、mermaid 五组的并集（29 个），两者都未出现并集外身份。

## 禁库入边与原生目标路径

新增禁库入边全部是 `kind: normal`、`target: null`：

| 父包 | 子包 |
|---|---|
| `fancy-regex@0.16.2` | `regex-automata@0.4.18` |
| `fancy-regex@0.16.2` | `regex-syntax@0.8.11` |
| `syntect@5.3.0` | `regex-syntax@0.8.11` |
| `mermaid-rs-renderer@0.3.1` | `regex@1.13.1` |

基线本身已有三条获批路径入边，不计入“新增”：`getrandom@0.2.17 → wasi@0.11.1`（`cfg(target_os = "wasi")`）、`getrandom@0.3.4 → wasip2@1.0.4`（`cfg(all(target_arch = "wasm32", target_os = "wasi", target_env = "p2"))`）、`gpui-pre@0.3.9 → regex@1.13.1`（无条件）。

`docx-rs` 启用 `image` 时出现绕过获批根的 `wasip2@1.0.4+wasi-0.2.12` 路径（实测，绕开 `gpui-pre`/`gpui-pre-macos` 后的最短路径）：

```text
aline-plugin@0.1.0 --docx_rs[normal]--> docx-rs@0.4.22 --image[normal]--> image@0.25.10
  --ravif--> ravif@0.13.0 --rav1e[normal, normal@cfg(target = "wasm32-unknown-unknown")]--> rav1e@0.8.1
  --rand[normal@cfg(fuzzing)]--> rand@0.9.5 --rand_core--> rand_core@0.9.5
  --getrandom--> getrandom@0.3.4
  --wasip2[normal@cfg(all(target_arch = "wasm32", target_os = "wasi", target_env = "p2"))]--> wasip2@1.0.4+wasi-0.2.12
```

`cfg(fuzzing)` 与 wasm32/wasi 条件使该路径在任何桌面 target tree 中都不被选中（五个桌面场景 `selected = false`；`all`/`all-features` 为 `true`，因为 `--target all` 会把目标分支一起选上）。门禁仍在原生路径层拒绝，因为原生路径检查先于 metadata-only 旁路。结论：**`docx-rs` 生产接线必须 `default-features = false`**；Word 图片改用 `Pic::new_with_dimensions` + `tiny-skia` 预编码 PNG。

### 守卫扩展的实现（2026-11-26）

只把四条入边写进策略数据不够：旧守卫先判断“禁库是否可在 GPUI 之外到达”，`syntect`/`mermaid` 不经过 GPUI，因此在读例外表之前就被拒绝（实测仍报 `guard-bypass`）。按裁决对 `scripts/engineering_dependencies.py` 做了最小改动：

- 新增 `registered_outside_exception()`：禁库包在 GPUI 之外可达时，放行条件是**全图每一条入边**都在该身份的 `exceptions.incoming` 中，且来源、校验和、目标组合一致。
- 两处调用：metadata-only 禁库旁路检查与已选禁库旁路检查。`gpui`/解析器/无 GPUI 的图保持原有严格拒绝；任何未登记父包（包括 metadata-only 边）仍然失败关闭。
- 策略登记：`regex@1.13.1` 增加 `mermaid-rs-renderer@0.3.1` 入边；`regex-automata@0.4.18` 增加 `fancy-regex@0.16.2`；`regex-syntax@0.8.11` 增加 `fancy-regex@0.16.2` 与 `syntect@5.3.0`。七场景全部允许。

## GPUI 受审子图漂移

- `zip` 与 `docx-plain`：`flate2@1.1.10` 的 features 增加 `any_zlib`、`zlib-rs`，并**把基线已有但未被选中的 `zlib-rs@0.6.8` 变为选中**。`zlib-rs` 不是新增包，但 `selected` 翻转同样改变指纹，不能读成“只多了两个包”。
- `docx-*`：桌面场景另外把 `bumpalo@3.20.3`（`zopfli` 依赖）与 `rustversion@1.0.23`（`encoding_rs` 构建依赖）变为选中；`all`/`all-features` 已在基线选中这两者，故不漂移。
- `krilla`：新增 GPUI 子图行 `bytemuck_derive@1.12.1`；`bytemuck`、`float-cmp`、`smallvec`、`stable_deref_trait`、`yoke`、`zerofrom` 的 features/边变化；桌面场景另加 `bumpalo@3.20.3`（krilla 直接依赖）。
- `syntect`：`regex-automata@0.4.18` features 增加 `dfa`、`dfa-build`、`dfa-search`、`nfa`、`perf`（syntect 的 `regex-fancy` 组合）。
- `mermaid`：`fontdb@0.23.0` features 增加 `default`。
- `latex`：**零漂移**，指纹与基线完全相同；唯一拒绝是“已选包未进入运行时准入分组”。

拟批准指纹（**仅对表中该组精确 features 有效**；生产 manifest 实际组合不同则必须重跑探针重冻）：

| 分组 | all | macos-arm | macos-intel | windows-x64 | linux-x64 | linux-arm | all-features |
|---|---|---|---|---|---|---|---|
| zip | `333f7462fcd7b2e6539412763bb8c2b0e5e4fea440ec42b056dc37cf54f0eafd` | `0f3036a6c87cd32a67749f0e0051ce6bbe44d453e60ba62991733320b1c73fa0` | `4d4d481d3621a7a4eddf547e5b600f6fb1639b714899ab006dccb632a22170a1` | `38ca42b2caf76535bd7df6cfee53e37a88b83497a67612ce4e595365e4c8d4b1` | `2e18494f9cec0d7cfa0cc61630e4723a9ee0c2a436d284b1e7588d792376e5a8` | `b83a626b65a64ee70d798403e07a80c1f6df5d4d5880fbe1eb4b0d76371c7ef4` | `333f7462fcd7b2e6539412763bb8c2b0e5e4fea440ec42b056dc37cf54f0eafd` |
| krilla | `b9e94fbe3e06c4f5ae3845a05c908911992f4383c12f89974cc218f09fb316f1` | `8c87e9092775dd416d467f700e6b12c1dfdafb997783cbbacaf887f4c4aeb74d` | `2fc88691333dd4f88935cbdda49286855c96fd6258385bfcff989c4d56613e6b` | `5cf52d38b44da222d10160515d4903152017ece5e85fc0be9292d9249d0f2352` | `052debd9c6b10b1dbb26df446370941414038f925e7d42f380ed9b982ae18273` | `082a87a39bf7b6ac9b0196b3dbac6be5f5586e71f4dac79658d9c57ff01900af` | `b9e94fbe3e06c4f5ae3845a05c908911992f4383c12f89974cc218f09fb316f1` |
| docx-plain | `333f7462fcd7b2e6539412763bb8c2b0e5e4fea440ec42b056dc37cf54f0eafd` | `3a9a3c3c3b47cbc936011db50601b33e318af094e17e5c07092694cdf34ca363` | `bff17d8749836cd267b91891e464dcdcc355d65d4e0f677f4b3bb123600fdc91` | `1563d9de9d8fdd18b0eafb76e2d97c1a290a8d7751c0194cc68fe78c720ec15e` | `4b19767821171e41f164ee53349510e6729bf62b3fc72afcae8b7d12981a5aba` | `faaa175233fdaf4d55fbaef935abf829c6e9bdc8f77670011b439c19948b3f6e` | `333f7462fcd7b2e6539412763bb8c2b0e5e4fea440ec42b056dc37cf54f0eafd` |
| syntect | `23e7b11d7151592f9d31b4173f6b7b0af8dd63567159f7dbdbedb2a30d7e6a3f` | `eb0d0bf65836d021a93d7858708b74376f4e70007d2e76bdb921347ba914c261` | `4b872bb269c4ae224c96a262880534647d45028c9f88e5f9fc7d7493e24921fe` | `05c299a99da4ff647491c0497083d74b6e712f81f49626cdc4cb17df61d4f975` | `dd00f463ac0a13677be944e92d6ae2732e49ea48ea6422db2797a4142975497a` | `96078cbb38ce64f09989c99f876c88531017ed451302146eaf84464f88bc33ed` | `23e7b11d7151592f9d31b4173f6b7b0af8dd63567159f7dbdbedb2a30d7e6a3f` |
| mermaid | `f6e77bbc287eaecd30441709fa9a9e8be86fd3c2bbac286aa4f4c9d85fb2cdfb` | `378aaf32fa7a7c4a848491cb431d9ceb87cfd610185550b7d818984e760c548e` | `dae86251b92709597dc4c3d01fb3e77954cfa10bce3212aa0742d79aca96142a` | `f344f962ecf8e379b874db691bda2ecebdab0672bf3c5f9d39eebc53145906d1` | `4a24ad5bbba060b68d9ea9b4f1ef3f3a47a5898beb2adfdd58a36929bbf78a63` | `44bc8198d5a2019a1ae16d0f9557d8a4e7c7e6ff85d03821f83e0bbc952c0a2e` | `f6e77bbc287eaecd30441709fa9a9e8be86fd3c2bbac286aa4f4c9d85fb2cdfb` |
| full-plain | `3297801c0315f2044bb62033fc07bf40b1dac612e4152bcec3ef6957402b0063` | `cbe723635342060091b959428e2388df5e87b0d8ba5291335fe5c9c88dacec1c` | `1715ddf8910eb9d6e5cf814a932347b7c8991b1b297d7e99b288575bbf52c2a1` | `d10ed843dcd512e7e89f0f647aef266e1059916cbac1f80fe4733fff722b8ae7` | `ecbd460920890d44c3916ab9763cdb07e59067922a1eaf75f89ac3bf2910da39` | `15f1e83afe8179b645a0fdaaf0d20c3a0f05766ebf80ed67745eefde7d3a54da` | `3297801c0315f2044bb62033fc07bf40b1dac612e4152bcec3ef6957402b0063` |
| production（终态） | `115f4b5f2c010a12267fe458659bbd117e81c60add5d620ace8559b35b362605` | `ee8fe5d8469b4e1a00643fb3885f885813d4c6e9d79c2aedba113cbac47b6551` | `edb4cd7ba1a790417e7c449f10b6fa134a8d2695175f64e3e310dc719e29108b` | `f05dac7bdcabb31c14339289907632e2b895aab015de2a613feb55ab4bece390` | `256e1366c6a60d4cd06d7350b3ec9cec7bd5d80715abba3ad62c1a88935c53f5` | `34a2bb67b27a8a5b752b2362eafee9f983e7ec70315a7cce5d83b695b8c7a821` | `115f4b5f2c010a12267fe458659bbd117e81c60add5d620ace8559b35b362605` |

`production` 的选包数为 all/all-features 424、macos-arm 363、macos-intel 365、windows-x64 318、linux-x64 310、linux-arm 308；metadata 464 包、新增 29 包，全部校验和/许可证/来源/rust-version 与冻结表一致。

**指纹登记时机：** 策略每场景只保存一个指纹，而指纹随清单实际组合变化，因此上表是“接线到该精确组合时应当登记的值”，不是现在就替换。本轮不修改 `engineering_policy.json` 的 `scenarios`；每个接线提交以其实际清单复跑探针，若与表中组合不一致，必须在复测后追加并经用户确认才能替换。

**已登记（2026-11-27，任务 4.1 接线）：** 生产清单仅新增 `aline-plugin → syntect@5.3.0`（冻结 features），与本表 `syntect` 组合一致；七个指纹已按上表登记进 `engineering_policy.json`，并以实际清单运行 `python3 scripts/check_engineering.py` 七场景全部通过。实图漂移仅为 `regex-automata@0.4.18` 的 features 增加 `dfa/dfa-build/dfa-search/nfa/perf`（`fancy-regex` 入边），无新增/移除包、无来源/校验和/边/选包变化。

`docx-image` 不列入拟批准：`image` feature 会引入获批根以外的路径（见上节），生产组合固定为 `docx-plain`。

## 运行时准入分组登记

裁决前 `latex` 组的唯一拒绝是 `已选包未进入运行时准入分组: latex-rust@2.1.1`。现已按裁决在 `scripts/dependency_admission_policy.json` 登记五个 `optional` 闭包分组（根身份、checksum、根入边、场景全部精确）；根不存在时自动跳过，出现即做精确核对：

| 已登记分组 | 根身份 | 覆盖包 |
|---|---|---|
| `export-pdf` | `krilla@0.8.2` | krilla 闭包 10 包 |
| `export-docx` | `docx-rs@0.4.22` | docx-rs 闭包 9 包，含 `zip`/`typed-path` |
| `math-latex` | `latex-rust@2.1.1` | latex-rust |
| `diagram-mermaid` | `mermaid-rs-renderer@0.3.1` | mermaid、json5、ucd-trie |
| `highlight-syntect` | `syntect@5.3.0` | syntect 闭包 6 包（regex 族仍归 gpui 组） |

`theme-zip` **本轮不登记**，原因不是裁决未通过，而是机制冲突：`zip` 会随 `docx-rs`（4.4）先出现，而闭包分组的 `root_incoming` 是精确集合——先只登记 `aline-plugin → zip` 会在 4.4 的中间状态失配，登记两条又会在只有一条时失配。因此 `zip`/`typed-path` 在 4.4 起由 `export-docx` 闭包准入；5.2 加入 `aline-plugin → zip` 直依赖时再登记 `theme-zip`，`root_incoming` 同时包含两条边，并用探针复测。分组归属是“先登记先得”，与 `gpui`/`gpui-macos-backend` 的共享包处理一致。

## 平台适配（零新增依赖）

行号是 2026-11-26 在锁定 GPUI 源码中的位置；`dependency: None` 表示不新增任何包。

| 能力 | 任务 | API | GPUI 位置 | macOS 实现 | 新增依赖 |
|---|---|---|---|---|---|
| 打开 http/https/mailto 链接 | 3.4 | `App::open_url` | `vendor/gpui-pre-0.3.9/src/app.rs:1809` | `gpui-pre-macos-0.3.9/src/platform.rs:831` | 无 |
| 在文件管理器中揭示文件 | 3.4 | `App::reveal_path` | `app.rs:1913` | `platform.rs:1023` | 无 |
| 用系统默认应用打开 | 3.4 | `App::open_with_system` | `app.rs:1918` | `platform.rs:1043` | 无 |
| 选择已存在文件/目录 | 3.4、5.2 | `App::prompt_for_paths` | `app.rs:1891` | `platform.rs:898` | 无 |
| 选择新文件路径 | 4.4、5.2 | `App::prompt_for_new_path` | `app.rs:1904` | `platform.rs:946` | 无 |
| 读取窗口外观 | 5.4 | `App::window_appearance` | `app.rs:1698` | `platform.rs:800` | 无 |
| 设置/跟随系统外观 | 5.4 | `App::set_window_appearance` | `app.rs:1712` | `platform.rs:808` | 无 |
| 观察外观变化 | 5.4 | `Window::observe_window_appearance` | `src/window.rs:2314` | `platform.rs:800` | 无 |

## 裁决结果（2026-11-26）

| 事项 | 裁决 | 执行状态 |
|---|---|---|
| 导出闭包（krilla/docx-rs/zip） | 全部批准 | `export-pdf`/`export-docx` 已登记；`theme-zip` 按上节推迟到 5.2；GPUI 指纹待接线提交登记 |
| `latex-rust` 内嵌素材 | 批准并随附许可证 | `math-latex` 已登记；分发义务见下 |
| 禁库守卫扩展（mermaid/syntect） | 两者都批准 | 守卫代码与四条入边已落地，七场景通过 |

**`latex-rust` 分发义务（必须随产品分发执行）：** 内嵌 `fonts/stix-two-math/STIXTwoMath-Regular.otf`（OFL-1.1）须随附 OFL 文本，不得单独售卖字体，不得用保留字体名发布修改版；`data/dvipsnames.tsv`（LPPL-1.3c）须保留文件头与 `NOTICE` 的修改声明，并保留获取原始 graphics bundle 的指引（`NOTICE` 的版权人行是未替换的模板占位符，不转录其内容）。这两项不在 SPDX 白名单内，是用户裁决的精确例外，不扩展全局许可证集合。

**仍未闭合的一项：** 4.1 的高亮语言集尚未在任何工件中冻结（见“契约缺口与更正”第 1 条）。它不改变已测量的依赖身份，但决定 `syntect` 发布全量 `default-syntaxes` 还是受控子集，必须在 4.1 开工前补齐冻结清单；若届时需要 `default-syntaxes` 之外的语法集（如 `two-face`），属于新增依赖，须重新准入。

## 当前门禁状态说明

本轮改了 `scripts/engineering_dependencies.py`（守卫放行条件）与两份策略文件，未改任何 Cargo 清单。`python3 scripts/check_engineering.py` 通过，证明现有八成员图仍满足全部规则；探针在工作区副本上的十一个注入组合也全部符合期望。两者都不等于生产组合已接入：GPUI 指纹仍是 M1 值，生产清单一旦变化就会失败关闭，必须按接线提交重新登记。

## 裁决结果（2026-11-27）：4.1 高亮语言集冻结

用户裁决：采用 `syntect@5.3.0` `default-syntaxes` 随 crate 内嵌的全量默认集作为 V-03 的高亮支持集；不引入 `two-face`，不在构建期生成受控子集 dump。本裁决闭合上文“仍未闭合的一项”。

- **身份**：`syntect@5.3.0`（校验和见上表）；内嵌 `assets/default_newlines.packdump` 368,467 字节、SHA-256 `d740b20c12e40b678b9f1012401e1969aaa5cd55f1ab329ffeb94d746b06a5c0`；共 **74** 个语法定义。
- **支持清单**（`包/名称`，按字母序，逐项来自该 dump）：ASP/ASP；ASP/HTML-ASP；ActionScript/ActionScript；AppleScript/AppleScript；Batch File/Batch File；C#/Build；C#/C#；C++/C；C++/C++；CSS/CSS；Clojure/Clojure；D/D；Diff/Diff；Erlang/Erlang；Erlang/HTML (Erlang)；Go/Go；Graphviz/DOT；Groovy/Groovy；HTML/HTML；Haskell/Haskell；Haskell/Literate Haskell；Java/Java；Java/Java Server Pages (JSP)；Java/JavaDoc；Java/JavaProperties；JavaScript/JSON；JavaScript/JavaScript；JavaScript/Regular Expressions (JavaScript)；LaTeX/Bibtex；LaTeX/LaTeX；LaTeX/LaTeX Log；LaTeX/TeX；Lisp/Lisp；Lua/Lua；Makefile/Make Output；Makefile/Makefile；Markdown/Markdown；Markdown/MultiMarkdown；Matlab/Matlab；OCaml/OCaml；OCaml/OCamllex；OCaml/OCamlyacc；OCaml/camlp4；Objective-C/Objective-C；Objective-C/Objective-C++；PHP/PHP；PHP/PHP Source；Pascal/Pascal；Perl/Perl；Python/Python；Python/Regular Expressions (Python)；R/R；R/R Console；R/Rd (R Documentation)；Rails/HTML (Rails)；Rails/JavaScript (Rails)；Rails/Ruby Haml；Rails/Ruby on Rails；Rails/SQL (Rails)；Regular Expressions/RegExp；RestructuredText/reStructuredText；Ruby/Ruby；Rust/Cargo；Rust/Rust；SQL/SQL；Scala/Scala；ShellScript/Bash；ShellScript/Shell-Unix-Generic；ShellScript/commands-builtin-shell-bash；TCL/HTML (Tcl)；TCL/Tcl；Textile/Textile；XML/XML；YAML/YAML。
- **拒绝清单**：不在上述定义中的语言标识（含空标识）与识别失败一律按 PLUGIN-05 回退普通代码——不显示拒绝提示、不阻止打开、不改变围栏 info。
- **识别口径**：围栏 info 与 syntect 语法定义内的名称/别名匹配；4.1 以探针枚举实际可识别的名称与别名并留证，不以文件扩展名猜测代替。
- **未纳入**：TypeScript/TSX、Kotlin、Swift、TOML、SCSS/Sass/Less、Dockerfile 等默认集不含的语言；若后续产品需要，须新增 `two-face` 依赖并重新准入。
