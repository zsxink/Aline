# M0 源码行尾表示独立复核

日期：2026-10-31。范围：任务 4.2 的设计先决分析。本文是分析证据与待确认方案，不是规范修订；未修改 proposal/specs/design/tasks，未实现 source，也没有 source 测试通过的结论。

## 1. 推荐结论

首选的“解码 CRLF→LF、保留孤立 CR”表示不能直接冻结。删除 `CR X LF` 中的 X 后，当前 String 是 `"\r\n"`；再次按该解码规则打开后为 `"\n"`，两者不严格相等。

**推荐提交用户确认的最小方案**：编辑源码保留原始 CR/LF 字节（仅分离头部 BOM）；编码将词法上的 CRLF 对或未配对 LF 视作行尾单元，按存储基线的多数派输出；将存储往返的“等价”限定为仅行尾表示等价。孤立 CR 及其余字符仍须逐字节保留。补丁、逆补丁、源码选区和历史的比较继续使用实际源码 String，不对 Buffer 做隐式规范化。

这一步必须确认：**编辑新拼接出的 CRLF 是否按一个行尾单元处理**。如认可，LF 策略下冻结源码 `"\r\n"` 重开为 `"\n"`，是获准的行尾表示变化；不能称它们严格 String 相同，也不能在确认前称其已通过“不丢字”。如要求该场景保留 CR 和 LF 两个独立源码字符，同时不准增加编码转义、拒绝普通编辑或改变行尾策略，现有简单字节格式无法满足这些要求。

## 2. 当前实现和约束

已确认事实：

- [引擎 §5.1](../../engine-design.md#51-解码与坐标边界)第 101 行与[架构 §5.1](../../v1-architecture.md#51-状态所有权)第 451 行把 CRLF→LF 明称为“建议表示”，允许另一表示；因此首选失败不能推出所有表示失败。
- 引擎第 105 行要求输入先确定表示再产生补丁/逆补丁，禁止事后无历史整理 Buffer、吞 CR 或自动补尾换行。
- 引擎第 112～119 行要求多数派编码、不吞孤立 CR、禁止无条件增加 CR 的替换；使用的原文是“再 decode 与冻结快照等价”，没有明确规定严格 String 相等。
- [PRD SAFE-03](../../v1-prd.md#52-保存保真与异常恢复)第 110 行规定统一行尾风格与未编辑内容保留，混合行尾多数派统一是明确例外。
- [source delta](../../../openspec/changes/m0-engine-foundation/specs/engine/source/spec.md)第 25 行使用“等价”，第 39～40 行另要求 CR-X-LF 拼接后内容一致、不能静默丢字符。[design](../../../openspec/changes/m0-engine-foundation/design.md#source-representation-gate)明确要求遇到此问题先评审表示和金样。
- 现有 `aline-source` 仍是职责注释骨架；本复核只运行 Python 字符串演算，不是对 Aline 解码器/编码器的测试。

推断：若把“等价”解释为严格 String 相同，又把新拼接的 CR 与 LF 视作两个必须分别保存的字符，则 LF 规范化解码的像空间对普通删除不封闭；只增加内存 provenance 不能让独立的 `decode(bytes, budget)` 在重开时恢复已丢失的区分。

未知/待确认：词法上新形成的 CRLF 是否纳入行尾表示等价；保存成功基线的相等证据怎样区分源码精确身份与编码等价。不能由实现代理静默选择。

## 3. 推荐方案与待确认措辞

建议确认后更新 source delta 的“保真编码与内容身份”、design Source Representation Gate、任务 4.1/4.2 的独立金样，并同步受影响的现行引擎 §5 / 架构 §5.1；proposal 若需解释范围也须同步。当前没有修改这些文件。

建议文本：

> 编辑源码采用不含头部 BOM、保留输入 CR/LF 字节的 UTF-8 表示。源码补丁与逆补丁按该实际表示精确执行。编码扫描时，CRLF 对和未配对 LF 各构成一个行尾单元；未配对 CR 是普通保留字符。行尾单元包括合法编辑在边界新形成的 CRLF。相对存储基线有变化时，只把行尾单元转换为基线多数派风格，平票 LF；不补尾换行，不更改其他字符。存储往返等价仅允许上述 CRLF/LF 表示差异，不要求重新打开后的源码 String 与编码前的 String 严格相等。此等价不得用于证明任意补丁无变化或修改历史坐标。保存成功的基线须绑定实际写出的 bytes、冻结源码身份与受检行尾等价关系；失败不更新基线。

用户需要批准的是这段**行尾语义**，尤其 `"\rX\n"` 删除 X 后的 `"\r\n"` 在 LF 策略下保存重开变为 `"\n"`。不需要新依赖、旁车文件、隐藏字符编码或第二份可编辑文本。

若用户不同意上述行尾语义，必须另行裁决标准 UTF-8 文件怎样无歧义表达“独立 CR 字符紧邻 LF 行尾”；不能依赖临时 provenance 解决重开信息丢失。

## 4. 关键取舍

- 原始源码表示使坐标始终针对实际 Buffer 字节；CRLF 本身占两个源码字节。磁盘和源码转换由 source 映射承担，不能在调用点做偏移算术。
- 混合行尾按既定策略输出时，重开的原始源码可以只在行尾表示上不同。原 bytes 通道、源码精确相等与存储行尾等价须区分，不能用宽泛等价放行错误补丁或假装文本严格相同。
- 本方案把新拼接 CRLF 解释为一个行尾单元。这是需要用户确认的语义边界；不把它包装成纯内部实现选择。
- 无条件 LF 扩展在 CRLF 策略下能得到 `0d0d0a` 并让规范化解码恢复 `"\r\n"`，但引入额外 CR，不符合已设的编码准入条件。

## 5. 风险和回滚方式

当前只新增本分析文档，没有改变接口或存储行为，可继续保留作为失败候选证据。用户确认之前暂停依赖此表示的接口冻结和 source/buffer 实现。若确认后验证失败，保留独立反例与日志，重新评审设计；不能通过悄悄规范化 Buffer、放宽断言或修改金样预期掩盖失败。

主要风险是把存储行尾等价误用到工作版本、历史和 dirty：例如用户只改行尾表示时，不能不经证据绑定便把它当作任意未修改源码。保存成功的实际 bytes 与冻结输入身份必须分别保留，Undo 不倒退磁盘基线。真实文件替换、权限与平台服务均未验证。

## 6. 验证计划与实际演算

实际环境：当前 macOS 主机；`python3 -V` 退出码 0，输出 `Python 3.9.6`。以下两条命令均退出 0，仅用标准库和纯字符串，不读写用户文件、不调用 Aline 实现。独立预期来源为手算 UTF-8/CR/LF 字节及已列规范，不能将输出录为 source 金样通过结论。

第一条完整命令：

```sh
python3 - <<'PY'
import json

def normalized(text):
    return text.replace('\r\n', '\n')

def encoded(text, policy):
    logical = normalized(text)
    return (logical if policy == 'LF' else logical.replace('\n', '\r\n')).encode('utf-8')

cases = [
    ('boundary-LF', 'LF', b'\rX\n'),
    ('boundary-CRLF', 'CRLF', b'a\r\nb\r\n\rX\n'),
]
for label, policy, original in cases:
    for representation in ['normalized', 'raw']:
        source = original.decode('utf-8')
        if representation == 'normalized':
            source = normalized(source)
        start = source.index('X')
        frozen = source[:start] + source[start + 1:]
        output = encoded(frozen, policy)
        reopened = output.decode('utf-8')
        if representation == 'normalized':
            reopened = normalized(reopened)
        inverse = frozen[:start] + 'X' + frozen[start:]
        print(json.dumps({'case': label, 'representation': representation,
                          'original_hex': original.hex(), 'source': source,
                          'delete_range': [start, start + 1], 'frozen': frozen,
                          'encoded_hex': output.hex(), 'reopened': reopened,
                          'strict_equal': frozen == reopened,
                          'eol_equal': normalized(frozen) == normalized(reopened),
                          'inverse_on_frozen_exact': inverse == source}, ensure_ascii=True))

for policy, original in [('LF', b'a\r\nb\nc\n'), ('CRLF', b'a\r\nb\r\nc\n')]:
    source = original.decode('utf-8')
    frozen = 'A' + source[1:]
    output = encoded(frozen, policy)
    reopened = output.decode('utf-8')
    print(json.dumps({'case': 'mixed-' + policy, 'representation': 'raw',
                      'original_hex': original.hex(), 'frozen': frozen,
                      'encoded_hex': output.hex(), 'reopened': reopened,
                      'strict_equal': frozen == reopened,
                      'eol_equal': normalized(frozen) == normalized(reopened)}, ensure_ascii=True))

frozen = '\r\n'
output = frozen.replace('\n', '\r\n').encode('utf-8')
print(json.dumps({'case': 'unconditional-LF-expansion-CRLF', 'frozen': frozen,
                  'encoded_hex': output.hex(), 'normalized_reopened': normalized(output.decode('utf-8')),
                  'strict_equal': frozen == normalized(output.decode('utf-8')),
                  'admissible': False,
                  'reason': 'Adds CR; contradicts the guarded storage policy, not an accepted solution.'}, ensure_ascii=True))
PY
```

关键输出（反斜线形式表示真实控制字符；`等价`列仅计算候选定义，没有得到用户批准）：

| 场景 / 表示 | 原 bytes hex | 删除 X 后冻结源码或首字母修改后源码 | 输出 hex | 重开源码 | 严格相等 | 候选行尾等价 | 冻结文本执行逆操作 |
|---|---|---|---|---|---|---|---|
| LF 边界 / 规范化 | `0d580a` | `"\r\n"` | `0a` | `"\n"` | false | true | 精确恢复 |
| LF 边界 / raw | `0d580a` | `"\r\n"` | `0a` | `"\n"` | false | true | 精确恢复 |
| CRLF 边界 / 规范化 | `610d0a620d0a0d580a` | `"a\nb\n\r\n"` | `610d0a620d0a0d0a` | `"a\nb\n\n"` | false | true | 精确恢复 |
| CRLF 边界 / raw | `610d0a620d0a0d580a` | `"a\r\nb\r\n\r\n"` | `610d0a620d0a0d0a` | `"a\r\nb\r\n\r\n"` | true | true | 精确恢复 |
| 混合 LF 多数 / raw | `610d0a620a630a` | `"A\r\nb\nc\n"` | `410a620a630a` | `"A\nb\nc\n"` | false | true | 本演算未执行逆操作 |
| 混合 CRLF 多数 / raw | `610d0a620d0a630a` | `"A\r\nb\r\nc\n"` | `410d0a620d0a630d0a` | `"A\r\nb\r\nc\r\n"` | false | true | 本演算未执行逆操作 |

最后一行独立反方案输出：冻结 `"\r\n"`，无条件 LF 扩展输出 `0d0d0a`，规范化重开 `"\r\n"`，严格相等 true，但 `admissible=false`。这不是准入成功。

第二条完整命令验证原首选在 LF 策略下直接保留 CR 后，仍不能解决规范化重开：

```sh
python3 - <<'PY'
import json
frozen = '\r\n'
output = frozen.encode('utf-8')
print(json.dumps({'case': 'literal-LF-output', 'frozen': frozen, 'encoded_hex': output.hex(), 'normalized_reopened': output.decode('utf-8').replace('\r\n', '\n'), 'strict_equal': frozen == output.decode('utf-8').replace('\r\n', '\n')}, ensure_ascii=True))
PY
```

实际输出：

```json
{"case": "literal-LF-output", "frozen": "\r\n", "encoded_hex": "0d0a", "normalized_reopened": "\n", "strict_equal": false}
```

用户确认后的实际 source 验证仍须执行：独立 bytes 金样、LF/CRLF/混合/平票/BOM、保留孤立 CR、边界插入/删除/粘贴、逆补丁原样恢复、源码/存储映射、旧/新基线相等证据及成功/失败回执。当前这些 Aline 测试均**未执行/未验证**；V-01～V-09 均不据本文判通过。

## 7. 不推荐方案及原因

- 简单规范化解码加保留 CR 编码：LF 边界反例可重现，重开吞掉该 CR。
- 只增加 provenance：重开只收到 bytes，没有足够信息区分本来 CRLF 与独立 CR 紧邻 LF；无法单独解决。
- 额外 CR 转义或隐藏字符：超出“仅 BOM/多数派行尾”的标准 UTF-8 编码约束，改变文件内容或 Markdown 语义。
- raw 源码却声称严格 String 往返：混合多数派样例已直接反驳；必须明确仅行尾表示等价。
- 拒绝删除/粘贴，或在 Buffer 外隐式删除 CR：缩减普通输入契约或修改未选内容，违反当前 gate。
- 在确认前直接选行尾语义并勾任务：将未裁决合同写成实现事实，不符合用户暂停条件。
