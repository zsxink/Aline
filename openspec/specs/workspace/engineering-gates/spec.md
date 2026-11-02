# workspace/engineering-gates Specification

## Purpose
为 M0 提供可复查的环境与依赖准入证据、八 crate 工程检查及出口记录，细化现有工程治理要求在当前阶段的接入方式，确保规划、负例验证和平台实验状态始终可区分。

## Requirements

### Requirement: 候选与闭包准入

M0 MUST 先登记兼容目标及工具/依赖候选的来源、版本、用途、许可证、feature/target 与完整闭包；无合规组合时 MUST 阻塞相关接线。关联 E1/E6、R-1、S-2/13/15。

候选 MUST 遵循现行架构 §3.2 已批准的 GPUI 0.3.9 精确例外，核对来源、版本、归档校验和、全部实际入边/根路径、target/features 和消费者。五引擎及自有直接依赖 MUST NOT 使用该例外。完整 metadata 图 MUST 保留并与实际 target/feature 的 Cargo tree 交叉核对；metadata 过近似路径 MUST NOT 自动算已启用或被静默丢弃，解析失败 MUST NOT 算通过。正式 workspace 可更换隔离消费方并增加合规节点，受审 GPUI 子图改变 MUST 重新审计。

#### Scenario: 合规候选取证

- **WHEN** 候选满足单一精确 GPUI 来源、单解析器及禁令/精确例外全部条件
- **THEN** 保存可重现闭包与工具证据后才允许创建实际依赖配置，不预装未使用后端

#### Scenario: 别名传递与目标违规

- **WHEN** 引擎经别名/dev/build/feature/目标依赖引入 GPUI，或任意闭包触及未获精确例外的禁用库、新引入路径、例外身份/校验和/目标条件漂移或反向内部边
- **THEN** 检查必须失败并展示真实包及路径；图解析失败不能算通过，不私自缩窄禁令

#### Scenario: 精确例外与旁路

- **WHEN** app/UI/plugin 消费锁定 GPUI 的既有 regex 路径，或完整图保留限定 WASI p2 绑定且全部桌面目标实际选包排除它
- **THEN** 只在完整身份/边/条件一致时允许；同一 regex 包被自有直接依赖、其它中间包或五引擎引入必须拒绝，不能因已有合法 GPUI 路径而全包放行

### Requirement: 有效门禁和双语资源

M0 MUST 为每成员证明 lint 策略有效，按生产/测试作用域验证安全规则，校验显式 locale 的双语键与插值；完整 Cargo 门禁 MUST 实际执行并记录。关联 E1、RS-01～05、K-01/06/08。

#### Scenario: 正例与最小违规反例

- **WHEN** 工程门禁完成接线并运行正常样本、成员漏继承、unsafe、引擎 unwrap/生产 panic 类出口、catalog 缺键或插值漂移反例
- **THEN** 正常样本通过，各反例在预期守卫失败并记录诊断及盲区，移除反例恢复通过，不以根配置存在代替证据

#### Scenario: 当前尚无 workspace

- **WHEN** 仅完成本次规划并验证 OpenSpec
- **THEN** 只报告规格结构结果，fmt/clippy/test、工程守卫、M0 出口和 V 实验均不标已通过

### Requirement: 逐项出口交接

M0 交付 MUST 对 E1～E7 逐项记录环境、夹具、命令、实际结果和缺口，并区分 AC 基础与完整验收；V-01～09 MUST 保持真实分项状态。关联架构 §13、R-17、S-12/19。

#### Scenario: 部分平台和基础结果

- **WHEN** 仅获得 macOS 无头测试及部分 V-06/V-07 基础结果
- **THEN** 只回填该环境和分项证据，保留 Intel Mac/Windows10/Linux、原生 IME、完整 AC/V 未测项；未满足出口时不宣称 M0 完成或开始依赖的 M1 接线
