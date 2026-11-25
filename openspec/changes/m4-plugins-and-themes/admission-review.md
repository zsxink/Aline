# M4 后端与平台适配准入审查

状态：**任务 1.5 未完成——三项准入决定待用户裁决。** 本记录冻结了 1.2～1.4 支持契约对应的候选后端、版本、校验和、许可证、依赖边、目标/features 与逐场景门禁裁决，并把它们做成可复跑探针（自测与十组实测均通过）。但下列三项必须由用户裁决后，相应生产接线才可开始：导出闭包（krilla/docx-rs/zip）与新 GPUI 指纹、`latex-rust` 内嵌素材许可证、`mermaid`/`syntect` 的禁库守卫扩展。在此之前不得把本记录读成“已准入”，也不得写 `engineering_policy.json`/`dependency_admission_policy.json` 或改生产 manifest。

已可准入（零新增包、零漂移）：`usvg@0.46.0`、`resvg@0.46.0`、`tiny-skia@0.11.4` 三个直依赖，以及 M4 全部 8 项平台适配（锁定 GPUI 既有 API，零新增依赖）。“依赖已明确”不等于对应任务已完成。

## 方法与可重复性

- 规则层 `scripts/m4_admission_rules.py`：纯常量与纯函数，冻结候选身份、直接依赖行、分组期望、内嵌素材、平台适配与策略快照 SHA-256；不读磁盘、不起进程。
- 探针 `scripts/probe_m4_admission.py`：把八个 workspace 成员、`vendor/` 与四份根清单复制到 `<tmp>` 工作区副本，按分组注入候选直接依赖，逐场景执行 `cargo metadata`/`cargo tree`，再用**门禁自己的** `engineering_dependencies.validate()` 裁决；同时用门禁自己的 `gpui_snapshot()`/`fingerprint()` 重算 GPUI 受审子图指纹，使被指纹检查拒绝的组也能给出拟批准值。
- 复跑命令：`python3 -I scripts/probe_m4_admission.py --self-test` 与 `python3 -I scripts/probe_m4_admission.py --offline`。本轮结果：`SELFTEST_OK groups=10 packages=29 scenarios=7`、`PROBE_OK groups=10 failed=0`。
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
| `zip@8.6.0` | `2d04a6b5381502aa6087c94c669499eb1602eb9c5e8198e534de571f7154809b` | MIT | 1.88 | `deflate-flate2-zlib-rs` | aline-plugin | 待裁决 |
| `krilla@0.8.2` | `27da593198b20eeba65caeb73c2bbeec3e53ab08fa549898312ce81c4fce5e33` | MIT OR Apache-2.0 | 1.92 | `raster-images, simple-text` | aline-plugin | 待裁决 |
| `docx-rs@0.4.22` | `7fdf00e8af6d0b3e92d4bbf9b76f773d8b84ea80f310324ad16cbdc2e653e02c` | MIT | — | 无 | aline-plugin | 待裁决 |
| `latex-rust@2.1.1` | `f8094b939c8af51f9c7d08256ffa607b743e24d466d9a0ccc602dc748c911e54` | MIT OR Apache-2.0 | 1.76 | `std` | aline-plugin | 待许可证裁决 |
| `mermaid-rs-renderer@0.3.1` | `6cb5b469222ce6b9896edf0a969cf68464f5a028ef394fd6643f00bd943df117` | MIT | — | 无 | aline-plugin | 待守卫裁决 |
| `syntect@5.3.0` | `656b45c05d95a5704399aeef6bd0ddec7b2b3531b7c9e900abbf7c4d2190c925` | MIT | — | `parsing, default-syntaxes, default-themes, dump-load, regex-fancy` | aline-plugin | 待守卫裁决 |
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

- 基线在七个场景全部通过，指纹与当前已批准的七场景值逐一相同；选包数为 all/all-features 394、macos-arm 332、macos-intel 333、windows-x64 286、linux-x64 278、linux-arm 277。
- `svg-direct` 在七个场景全部通过且指纹不变：三个直依赖已在锁定 GPUI 闭包内，仅增加调用点。
- 各组拒绝消息分类稳定，未出现“期望类别不符”或无法归类的消息。

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

`full-plain` 的新增包是 krilla、docx-plain、latex 三组的并集（20 个），未出现并集外身份。

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

`docx-image` 因用户未裁决不列入拟批准；其指纹与 `docx-plain` 相同，但依赖路径含上节原生分支，不应作为生产组合。

## 运行时准入分组缺口

`latex` 组的唯一拒绝是 `已选包未进入运行时准入分组: latex-rust@2.1.1`，说明现有 8 个分组未覆盖新闭包。若用户批准，需要为每个新闭包登记分组（root 身份、checksum、root 入边、允许场景），建议划分：

| 建议分组 | 根身份 | 覆盖包 |
|---|---|---|
| `export-pdf` | `krilla@0.8.2` | krilla 闭包 10 包 |
| `export-docx` | `docx-rs@0.4.22` | docx-rs 闭包 9 包 |
| `theme-zip` | `zip@8.6.0` | zip、typed-path |
| `math-latex` | `latex-rust@2.1.1` | latex-rust |
| `diagram-mermaid` | `mermaid-rs-renderer@0.3.1` | mermaid、json5、ucd-trie |
| `highlight-syntect` | `syntect@5.3.0` | syntect 闭包 6 包 |

同一包可能被多个根共享（`zip`/`typed-path` 同时属于 docx-rs 与 zip 闭包），分组规则必须按闭包并集处理，不能按“首次归属”独占。

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

## 待用户裁决

1. **导出闭包（`export-closure`）**：是否按上表精确身份、校验和、边与 features 登记 krilla/docx-rs/zip 闭包，新增 `export-pdf`/`export-docx`/`theme-zip` 分组，并采用探针重算的七场景 GPUI 指纹？不批准则 4.4 导出、5.2 主题包安装与主题 ZIP 导入无后端可用。
2. **`latex-rust` 内嵌素材（`latex-rust-assets`）**：是否准入内嵌 `fonts/stix-two-math/STIXTwoMath-Regular.otf`（838652 字节，OFL-1.1）与 `data/dvipsnames.tsv`（3212 字节，LPPL-1.3c，`NOTICE` 的版权人行是未替换的模板占位符）的 `latex-rust@2.1.1`？两者许可证都不在现行白名单内；批准须同时接受分发时随附 OFL 文本、保留 `NOTICE` 修改声明与原始 graphics bundle 获取指引。不批准则 4.2 公式无可锁定后端，须缩减 PRD 范围或另找候选。
3. **禁库守卫扩展（`regex-guard`）**：是否为 `mermaid-rs-renderer`/`syntect` 扩展禁库守卫、登记上表四条精确入边并重定 GPUI 基线？这需要改 `engineering_dependencies` 的守卫代码（把精确例外覆盖到这两个根），不是只加策略数据。不批准则 4.1/4.3/4.5 的图表与代码高亮无后端，须缩减 PRD 范围。

另有一项独立于上述三者的待确认：**4.1 的高亮语言集尚未在任何工件中冻结**（见“契约缺口与更正”第 1 条）。它决定 `syntect` 是发布全量 `default-syntaxes` 还是受控子集，应在 4.1 开工前补齐冻结清单。

## 当前门禁状态说明

本轮未改任何 Cargo 清单或策略文件。因此 `python3 scripts/check_engineering.py` 若通过，只证明**现有清单没有变化**，不构成本记录任何候选的准入证据；真正的准入要在用户裁决并完成策略登记后，用同一探针与正式入口复跑。
