# F-18：既有提交 scope 偏差登记

按 RS-05（`AGENTS.md`，提交纪律）：“纯文档用 `docs`，公共工程配置/规则用 `workspace`”。提交 `16f8e30` 的实际 `git show --stat` 输出为：

```text
commit 16f8e3007fed9a6de9f2ef8258d7ddb99624d3df
Author: xian <zsxink@gmail.com>
Date:   Sat Oct 31 22:22:06 2026 +0800

    docs(workspace): 按 round-4 独立复判勾选任务 6.8 与 7.4

 openspec/changes/m0-engine-foundation/tasks.md | 4 ++--
 1 file changed, 2 insertions(+), 2 deletions(-)
```

该提交只改 OpenSpec tasks 勾选，属于纯文档内容，却使用 `docs(workspace)`，因此登记为 RS-05 scope 偏差。依据 `git show --stat 16f8e30` 原始输出，详见 `scope-deviation-show.stdout.log`。本登记不重写历史、不改变 RS-05 决定，也不为该偏差制造新提交。
