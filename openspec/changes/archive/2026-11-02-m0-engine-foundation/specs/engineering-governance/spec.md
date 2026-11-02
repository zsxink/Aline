# Spec Delta

## MODIFIED Requirements

### Requirement: crate 依赖与引擎隔离必须可核查

实现 MUST 保持现行八 crate 的单向允许依赖及 source/app-core 独立边界。五引擎 crate MUST 保持完整依赖闭包无 GPUI，不持窗口、执行器、线程或全局状态。新增 crate、依赖或公开契约 MUST 记录架构影响并按准入流程处理。

禁用依赖 MUST 按现行架构 §3.2 的精确例外核对来源、版本、归档校验和、全部引入路径、target/features 与消费者；五引擎 MUST NOT 借用 GPUI 例外。检查 MUST 保留完整图并与实际组合 Cargo tree 交叉核对，解析失败 MUST NOT 算合规。新增路径或例外身份漂移 MUST 重新审计，不能按前缀或整个 GPUI 闭包放行。

#### Scenario: 引擎通过依赖别名间接引入 GPUI

- **WHEN** 引擎的 target、feature、dev/build 或传递依赖经别名指向 GPUI
- **THEN** 判定违反隔离要求；M0 建立的依赖检查须识别真实 package 与闭包并拒绝该变更

#### Scenario: 提出新增 crate

- **WHEN** 模块需要独立为新的 crate
- **THEN** design 说明职责、消费方、依赖图和独立验证收益，不能只以行数或目录对称为依据

