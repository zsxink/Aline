# 原始行尾表示的确认与同步（2026-10-31）

用户在收到源码表示独立复核与具体推荐后答复“采用”，并要求继续完成全部任务。此前的 [复核文档](source-representation-review.md)和 [暂停记录](group-2/pause-report.md)保留当时尚待确认的历史状态；本记录解除该项契约阻塞，不宣称实现或验证通过。

确认内容：编辑源码分离头部 BOM、保留原始 CR/LF；编码将 CRLF 对和未配对 LF 视为行尾单元，包括合法编辑在边界新形成的 CRLF，按读取基线的多数派输出，平票 LF；未配对 CR 及其他字符保留。存储往返仅允许上述行尾表示差异，补丁、逆补丁、选区、历史、工作版本和无变化判定仍基于实际源码精确身份。例如 `"\rX\n"` 删除 X 得 `"\r\n"`，LF 策略重开 `"\n"`，不得声称二者 String 严格相同。

已同步 change 的 proposal、source delta、design Source Representation Gate、tasks 4.1/4.2，以及现行引擎 §5、架构 §5.1；没有改变 AC 编号或 V-01～V-09 口径。独立分析再次只读核对上述六文件，未发现新增契约阻塞。后续任务 3/4 仍须实际实现、独立金样与错误/旧基线反例，当前均未验证。

同步过程第一次大段 Python heredoc 在解析阶段以非 UTF-8 SyntaxError 退出 1，未执行写入；改用分段 apply_patch 完成同步。随后 `openspec validate m0-engine-foundation --strict --no-interactive` 退出 0，仅为规格结构校验。

后续实现采用显式所有者与不可伪造基线证据：实际磁盘 bytes、冻结源码身份和受检行尾等价分别保存；成功写出建立新基线，失败不更新，旧证据不能跨基线复用。此实现建议已在 design 冻结语义边界，具体类型与命令仍由任务 3/4 验证。
