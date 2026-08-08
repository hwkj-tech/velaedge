# VelaEdge Runtime 工业协议路线图

## 交付目标

VelaEdge Runtime 建立统一、可查询、可监控的工业协议适配体系。每个协议必须完成配置建模、真实连接、数据采集、异常恢复、运行指标、云端下发和端到端测试后，才能标记为“生产可用”。

## 当前能力基线

| 协议 | 传输 | 当前状态 | 已有能力 | 主要缺口 |
| --- | --- | --- | --- | --- |
| Modbus TCP | TCP | 部署候选（强化中） | 四类数据区批量读取、FC05/06/15/16 写入、U/I/F 16-64 位编码、字节序/字序、缩放偏移、寄存器位提取、细分质量码、采集超时与有界重连、连接级共享熔断、独立容器真实 socket、故障注入恢复、可选真实 MQTT PUBACK/RocksDB 证据与机器报告 | 物理设备 24 小时现场验收 |
| Modbus RTU | 串口 | 部署候选（强化中） | 四类数据区批量读取、FC05/06/15/16 写入、U/I/F 16-64 位编码、字节序/字序、缩放偏移、寄存器位提取、细分质量码、采集超时与有界重连、连接级共享熔断、真实 PTY 串口测试、点位探测 | 24 小时现场验收 |
| DL/T 645-2007 | 串口 | 部署候选（读取） | 抄表请求、校验、同总线多表串行轮询、重复请求合并、单表通信/协议/解码故障隔离与质量指标、16 个常用 DI 模板、标准响应长度校验、厂商 DI 响应长度契约、Cloud/Console 配置、真实 PTY 串口测试 | 多厂商设备 24 小时现场验收；写操作另行评估 |
| IEC 60870-5-101 | 串口 | 部署候选（读写） | 非平衡召唤、常用遥信遥测、CP24/CP56 时标、单点/双点/短浮点控制、SBO、Cloud/Console 配置、产品物化、PTY 测试 | 多厂商链路状态机互操作、长稳与现场验收 |
| IEC 60870-5-104 | TCP | 部署候选（读写） | STARTDT、长连接复用、总召唤、自发上送、常用遥信遥测、质量码、CP56 时标与固定站端时区、C_SC_NA_1/C_DC_NA_1/C_SE_NC_1 控制、可选 SBO、正确认校验、MQTT 指令编排、Cloud/Console 配置、Runtime 指标、真实 TCP 集成测试和独立容器 RTU 验收 | 多厂商互操作、自发上送长期运行、24 小时现场验收 |
| 自定义串口帧 DSL | 串口 | 部署候选（读取） | v1/v2 版本治理、Raw/SLIP/COBS 成帧、SUM8/XOR8/Modbus CRC16/CRC-16-CCITT-FALSE 校验、字段解码 | 目标设备 24 小时现场验收 |
| OPC UA Client | TCP | 部署候选（读写） | 匿名/用户名/X.509 认证、安全策略与消息模式、PKI 信任、长会话、批量 Read、原生 Subscription/MonitoredItem、断链缓存保护、NodeId、结构化 BrowsePath 与批量 Translate、StatusCode/时间戳映射、有界 Browse 点位发现、显式可写点位、精确 Built-in Type、标准 Write 服务、指令回读、超时与自动重连、真实服务集成测试、Cloud 配置与 Runtime 指标 | 安全端点互操作矩阵和 24 小时现场验收 |
| BACnet/IP | UDP | 部署候选（读写） | B/IP 长会话、Who-Is 发现、ReadPropertyMultiple 批量读取与单点降级、BBMD/Foreign Device 跨网段注册与自动续租、COV 订阅/续租/通知与轮询降级、对象/属性结构化配置、模拟量/二进制/多状态可写对象、1–16 写优先级、独立 MQTT 指令编排、质量映射、真实 UDP 集成测试及独立容器设备验收 | 24 小时现场验收 |
| Siemens S7 | TCP | 部署候选（读写） | S7-1200/1500 通信、DB/M/I/Q 地址、BIT/BYTE/WORD/DWORD/INT/DINT/REAL、长连接复用、`read_multi_vars` 批量读取、读写权限约束、真实 TCP 服务测试、Cloud/Console 配置与 Runtime 指标 | 多 PLC 型号互操作和 24 小时现场验收 |
| Omron FINS | UDP/TCP | 部署候选（读写） | FINS/UDP 与 FINS/TCP 长会话、TCP 节点握手与自动节点分配、CIO/WR/HR/DM/AR 字与位地址、相邻字与位点位批量读取、整数/浮点/布尔读写、双字字序、网络/节点/单元路由、超时重建、真实 UDP/TCP 报文测试、Cloud/Console 配置与 Runtime 指标 | 多 PLC 型号互操作和 24 小时现场验收 |

`Simulated` 只用于测试，不计入工业协议覆盖率。MQTT 是上行数据输出，不属于南向采集协议。

Runtime 健康 API 的协议成熟度与本表保持一致：`laboratory` 表示仅用于实验室验证，
`deployment_candidate` 表示真实适配器、配置、指标和自动化门禁已具备但仍缺现场长稳或厂商互操作，
`production` 表示已经完成全部生产验收，`planned` 表示尚不可执行。成熟度不再充当适配器开关；
实验室和部署候选适配器仍可执行，但控制面与现场运维不会再把它们误读为已经完成生产签署。

## 分阶段计划

### M0 协议框架与能力真值

- 建立 Runtime 协议注册表，统一协议 ID、显示名称、传输方式和成熟度。
- 健康 API 输出协议能力、读写能力和自动探测能力。
- Runtime 只上报真正可执行的协议，不再把配置占位协议当作已支持。
- 为每个生产协议建立连接、采集、超时、错误和重连指标。

### M1 Modbus 生产强化

- RTU/TCP 统一支持线圈、离散输入、输入寄存器和保持寄存器。
- 支持 FC05/FC06 单点写入，以及 FC15/FC16 连续地址批量写入和严格响应校验。
- 点位访问权限默认只读，Runtime 仅允许显式可写点位执行命令。
- 按站号、功能码和连续地址自动合并批量读取，严格遵守 2000 bit/125 register 上限。
- 支持字节序、字序、缩放、位域和质量码。
- 采集编排使用 `timeout_ms` 与 `retry_count` 执行有界超时、指数退避和连接重建，并记录错误、超时和重连指标。
- 增加设备级熔断；写命令继续经过来源授权、范围、速率、幂等、回读和审计策略。
- 使用 TCP 模拟器、PTY 和真实设备样例完成验收。

当前自动化完成项：连续点位批量读取、FC15/FC16 批量写、Runtime 指令编排批量落站、短暂超时后的连接重建，以及 Cloud 到 Runtime 一致的寄存器编码、字节序、字序、工程缩放和只读位域配置。协议连接现已支持 Cloud 可配置的连续失败阈值、开路冷却时间和半开恢复阈值；采集与 MQTT 下行指令在 daemon 进程内共享熔断状态，Runtime 重建采集执行器不会清空累计状态，健康指标暴露开路次数、拒绝次数和连续失败数。细分质量码覆盖通信失败、超时、协议异常、解码失败、配置错误、停止服务、协议不确定、沿用旧值、超量程、替代值和溢出；算法节点继承输入质量，MQTT 同时输出大类 `quality` 与细分 `quality_code`，Runtime 健康页和 Cloud 监控显示最近质量码及 Good/Uncertain/Bad 累计值。实验室门禁还会启动独立 Modbus TCP 容器、在运行中停止并恢复端点，并可连接真实 MQTT broker 验证 QoS 1 PUBACK 与 RocksDB outbox 清空。该证据始终标记为非物理设备；剩余现场门槛是目标资产 24 小时稳定性报告与 broker 侧消费凭证。

Modbus 寄存器点位使用结构化解码契约：`encoding` 支持 `u16/i16/u32/i32/u64/i64/f32/f64`，`byteOrder` 定义单个 16 位寄存器内字节序，`wordOrder` 定义多寄存器字序，`scale` 与 `offset` 执行 `engineering = raw * scale + offset`。写入时 Runtime 严格执行逆变换。`bitIndex` 支持从 16 位寄存器提取布尔值；在实现 FC22 原子掩码写之前，位域点位只允许只读。

下行 MQTT 指令、可视化配置和审计按
[指令编排设计](./command-orchestration-design.md) 独立交付。

### M2 OPC UA

- [x] 支持匿名、用户名密码和 X.509 用户证书认证；密码仅通过 Runtime 环境变量引用。
- [x] 支持 SecurityPolicy/MessageSecurityMode、PKI 目录、未知证书信任和证书校验。
- [x] 支持长 Session 复用、连接/请求/会话超时、会话重试和失效后重建。
- [x] 支持标准 NodeId 与单次服务调用批量 Read。
- [x] 映射 OPC UA StatusCode、SourceTimestamp 和 ServerTimestamp，并进入统一质量指标。
- [x] Cloud 新建/修改/校验/版本化下发、Runtime 执行、真实 OPC UA 服务集成测试和前端专属表单。
- [x] 使用原生 Subscription/MonitoredItem 按点位采样周期接收变化，并用低频 Read 探针阻止断链后继续输出陈旧缓存。
- [x] 增加有界 Browse 点位发现：支持根 NodeId、最大深度、标准命名空间开关、ContinuationPoint、标量类型推断、Cloud 调度和管理页真实结果展示。
- [x] 增加结构化 BrowsePath 与批量 TranslateBrowsePathsToNodeIds；解析结果按配置和会话缓存，拒绝歧义、远端及未完成目标。
- [x] 增加独立 MQTT 指令编排写入：只允许显式 `read_write`/`write_only` 点位，Cloud/Console 必须配置精确 OPC UA Built-in Type，Runtime 复用 Session 调用标准 Write 服务并支持可选 Read 回读校验。
- [x] 使用进程内真实 OPC UA 服务完成 `UInt16` 变量 Write/Read、完整 `ConfiguredEdgeRuntime` 指令图执行以及产品发布到边端配置物化验收。
- [ ] 完成安全端点互操作矩阵和 24 小时现场验收后升级为生产可用。

OPC UA 可写点位的 `writeDataType` 当前支持 `Boolean`、`SByte`、`Byte`、`Int16`、`UInt16`、`Int32`、`UInt32`、`Int64`、`UInt64`、`Float`、`Double` 和 `String`。配置校验会同时检查语义类型兼容性，Runtime 会执行整数范围与浮点有限值检查，不依赖服务端猜测 Variant 类型。NodeId 与结构化 BrowsePath 都可用于写入；BrowsePath 在当前 Session 内解析并缓存，Session 重建后重新解析。

### M3 电力与楼控

- [x] IEC 101 增加 CP24/CP56 常用遥信遥测时标解析和自动化 PTY 覆盖。
- [x] IEC 104 完成 Cloud 连接/点位配置、STARTDT、总召唤、会话复用、常用遥信遥测、质量/时标映射和真实 TCP 服务测试。
- [x] IEC 104 完成显式可写点位、单点/双点/短浮点控制、可选选择后执行（SBO）、匹配 IOA 的肯定激活确认、MQTT 指令图、影子更新、指标以及真实 TCP 报文字节验收。
- [x] IEC 104 增加独立容器 RTU，生产适配器已完成动态遥测、自发上送、会话复用、总召唤以及三类控制写后回读验收；双点整数遥信与命令统一保留标准 `1=OFF`、`2=ON` 状态码。
- [x] IEC 104 连接支持 `cp56TimeZoneOffsetMinutes` 固定站端偏移，Cloud、Console、产品物化和 Runtime 使用同一契约，并将 CP56Time2a 统一归一化为 UTC。
- [x] IEC 101 连接支持 `cp56TimeZoneOffsetMinutes` 固定站端偏移，Cloud、Console、产品物化和 Runtime 使用同一契约，并将 CP56Time2a 统一归一化为 UTC。
- [x] IEC 101 完成显式可写点位、单点/双点/短浮点控制、可选选择后执行（SBO）、匹配链路地址/公共地址/IOA 的肯定激活确认、MQTT 指令图、Cloud/Console 配置和生产串口 PTY 字节验收。
- [ ] IEC 101/104 完成多厂商互操作、自发上送长稳和 24 小时现场验收。
- [x] BACnet/IP 完成 B/IP UDP 会话、Who-Is 发现、ReadPropertyMultiple 批量读取与单点降级、Cloud/Console 结构化配置和真实 UDP 集成测试。
- [x] BACnet/IP 增加 BBMD/Foreign Device 跨网段注册、自动续租、Cloud/Console 配置和真实 UDP FDT 验证。
- [x] BACnet/IP 增加 COV 订阅、自动续租、变化通知、失败轮询降级和 Runtime 可观测指标。
- [x] BACnet/IP 增加模拟量、二进制和多状态可写对象，支持 1–16 写优先级、独立 MQTT 指令编排以及真实 UDP `WriteProperty`/`SimpleAck` 验收。
- [x] BACnet/IP 提供独立 Docker 设备实验室，以 directed Who-Is/I-Am、动态模拟量、ReadPropertyMultiple、COV、长会话和优先级写入回读验证生产适配器。
- [ ] BACnet/IP 完成 24 小时现场验收。
- [x] DL/T 645 增加同一 RS-485 总线多表串行轮询、重复请求合并、常用数据标识目录、Cloud 地址校验和控制台结构化模板选择。
- [x] DL/T 645 增加单表异常隔离：部分表超时、坏帧或解码失败时继续轮询其余表，只输出成功样本，并把失败点计入细分质量、超时和错误指标；全部表失败时整轮仍失败并进入既有重试与熔断流程。
- [x] DL/T 645 厂商扩展 DI 使用 `meter:data_identifier:decimal_places:value_bytes` 地址契约；Cloud 要求未知 DI 显式声明 1-251 字节响应值长度，Console 提供结构化编辑，Runtime 严格验帧并兼容既有三段标准地址。
- [ ] DL/T 645 完成多厂商设备 24 小时现场验收。当前点位保持只读；若后续开放写操作，必须进入独立指令编排并仅允许显式可写点位。

IEC 101 和 IEC 104 的 CP56Time2a 都按连接的 `cp56TimeZoneOffsetMinutes` 固定偏移解释后统一转为 UTC；默认值为 `0`，中国标准时间站端应配置 `480`，禁止依赖宿主机本地时区猜测。IEC 101 点位地址使用 `link_address:common_address:ioa`，例如 `1:2:1001`；IEC 104 使用 `common_address:ioa`，例如 `2:1001`。当前适配器为每条连接绑定一个公共地址；需要访问多个站端公共地址时应创建多条协议连接。可写布尔、整数和浮点点位必须分别显式配置 `C_SC_NA_1`、`C_DC_NA_1` 或 `C_SE_NC_1`，双点命令仅接受 `1=OFF`、`2=ON`，并可按点位启用 SBO。Runtime 只有收到同一地址的肯定激活确认后才更新影子；超时、否定确认或断链都会使指令失败并重建会话。

### M4 厂商协议

- [x] Siemens S7 完成 S7-1200/1500 基线：DB/M/I/Q 地址、常用数据类型、真实 TCP 读写、长连接复用、Cloud/Console 配置与 Runtime 指标。
- [x] Siemens S7 使用 `read_multi_vars` 在单次采集周期批量读取配置点位。
- [x] Siemens S7 提供独立 Docker 设备实验室，以动态 DB 数据、真实 ISO-on-TCP/S7 报文、长会话和命令回读验证生产适配器。
- [ ] Siemens S7 补齐多 PLC 型号互操作和 24 小时现场验收。
- [x] Omron FINS 完成 UDP 基线：CIO/DM/WR/HR/AR 字与位地址、整数/浮点/布尔真实读写、双字字序、路由地址、Cloud/Console 配置与 Runtime 指标。
- [x] Omron FINS 按内存区合并重叠或连续字地址，位点位复用整字读取，单窗口遵守 700 字上限并保持配置输出顺序。
- [x] Omron FINS 完成 FINS/TCP 节点地址握手、自动节点分配、长连接复用、读写与断线重连，老配置继续默认 FINS/UDP。
- [x] Omron FINS 提供独立 Docker 设备实验室，同时开放 FINS/TCP 与 FINS/UDP，以动态内存区、节点握手、长会话和命令回读验证生产适配器。
- [ ] Omron FINS 补齐多 PLC 型号互操作和 24 小时现场验收。
- [x] 提供西门子与欧姆龙内置产品模板，复用统一点位集、采集编排和独立指令编排模型；模板以发布版本进入产品目录，边端绑定后可直接生成包含协议专属连接参数、只读/可写点位、遥测图、MQTT 输出和安全写指令图的 `EdgeConfigPackage`，并覆盖 SQLite 旧目录增量补种与 API 绑定回归。

Omron FINS 点位地址使用 `CIO0.5`、`W10`/`WR10`、`H10`/`HR10`、`D100`/`DM100`、`A10`/`AR10`。位地址只允许布尔点位，DM 区按当前 FINS 客户端能力只提供字访问；整数映射一个无符号 16 位字，浮点映射连续两个字并由连接级 `wordOrder` 指定双字顺序。只有显式标记为 `read_write` 或 `write_only` 的点位才能进入独立指令编排并执行写操作。

## 协议完成定义

协议只有同时满足以下条件才可标记为生产可用：

1. Cloud 可创建、校验、版本化并下发协议配置。
2. Runtime 可建立真实连接并连续采集至少 24 小时。
3. 支持超时、断线、重连、坏点和部分失败。
4. 健康页可查看连接状态、延迟、错误、超时和重连次数。
5. 数据可进入计算节点并输出至多个 MQTT Topic。
6. 具备单元测试、协议模拟器测试和端到端配置下发测试。
7. 文档包含地址格式、限制、错误码和现场验收步骤。

仓库级软件验收使用 `scripts/run-protocol-matrix-acceptance.sh`。该门禁按协议分别运行真实
loopback TCP/UDP 服务或操作系统 PTY 测试，并保留结构化报告与独立日志；它用于证明生产
适配器、协议报文和 Runtime 执行链路没有回归，但不会替代厂商互操作或 24 小时物理设备验收。
schema v2 报告会记录实际测试目标、每个参与验证的源文件及其 SHA-256、聚合源码摘要和门禁脚本摘要，
使离线归档的报告能够复核“哪一版实现由哪些测试验证”，而不是只保留无法展开的单个摘要。每次测试调用
还会记录精确过滤器、独立日志、状态和实际执行数；过滤器命中零项时即使 Cargo 返回成功也会判定门禁失败。
Custom Serial v2 的矩阵项会在操作系统 PTY 上执行 SLIP 与 CRC-16/CCITT-FALSE 报文交换，经过
`ConfiguredEdgeRuntime` 解码、计算和组包，再由真实 TCP MQTT 会话完成 QoS 1 PUBACK；报告源码摘要同时绑定
配置契约、生产适配器、Runtime 执行器、协议目录和测试代码。v1 Raw 配置继续作为兼容能力保留。

S7/FINS/IEC 104/BACnet/IP 独立容器设备验收使用 `scripts/run-container-protocol-device-acceptance.sh`。它会启动
四台独立于测试进程的有状态设备、运行生产 Runtime 适配器完成动态采集、长连接或 UDP 会话、事件上送和写命令回读，并输出
机器可读报告。容器证据比进程内 loopback 更接近部署拓扑，但依然明确标记
`physicalDeviceExercised=false`。在隔离网络中已预构建 `velaedge/protocol-device-sim:0.1.0` 时，可设置
`EDGEOPS_CONTAINER_PROTOCOL_NO_BUILD=1` 禁止镜像构建和远程元数据查询；验收报告会记录实际执行的
镜像名称与不可变 image ID，并把脚本自身纳入源代码摘要。

现场长稳验收统一使用 `field-endurance`：工具直接读取发布的 `EdgeConfigPackage`，按产品采集
周期执行生产适配器、计算图、多 MQTT sink 和 RocksDB outbox，并输出点位变化、协议质量、
重连、PUBACK、失败率与物理资产身份的机器报告。物理模式禁止 `Simulated`、禁止跳过 MQTT，
但 `physicalDeviceExercised` 仍是现场操作员声明；没有目标厂商设备和 broker 侧消费凭证时，
该工具的存在本身不能把路线图中的“24 小时现场验收”标记为完成。具体命令与证据要求见
[现场验收文档](./field-acceptance.md#generic-product-package-endurance)。

物理设备收口统一使用 `field-interoperability-gate`。CLI 的兼容默认仍要求 DL/T 645-2007、IEC 101、
IEC 104 与 OPC UA 每个协议至少两家不同厂商；正式 `site` 发布自动加载
[`deploy/field-acceptance-policy.json`](../deploy/field-acceptance-policy.json)，把全部十类非模拟南向协议纳入同一矩阵。
策略可以为每个协议分别定义最少厂商数和最少型号数：S7 与 FINS 要求同厂商至少两个 PLC 型号，
DL/T 645、IEC 101/104 与 OPC UA 要求两个厂商/型号，Modbus、Custom Serial 与 BACnet/IP 先要求一个真实型号。
每个覆盖项都需要物理设备 24 小时 `field-endurance` 报告，并逐份校验配置摘要、时长、全局与逐连接失败率、点位、
协议连接、每协议采集尝试/成功计数、连接末态、熔断末态、MQTT PUBACK 与 RocksDB outbox；每条被启用数据配置
使用的协议连接及每个实际发布的 MQTT sink 都必须独立通过，不能由其他健康连接稀释失败率。
schema v4 证据还记录启动、相邻成功进展及结束尾段中的最大间隔和计数器回退；现场策略将采集与
PUBACK 的最大无进展时间限制为 300 秒，避免仅凭累计计数掩盖长时间停顿。仅在报告结束时在线但没有
成功采集活动的协议不会计入覆盖，重复报告和重复设备也不会增加覆盖数。每份报告必须同时提供
原始配置包、结构化 broker 消费回执和 broker 原生审计导出；门禁自行计算工件 SHA-256，核对 edge/version、发布总数、
多 broker 路由和实际 Topic，并把回执摘要写入最终矩阵。门禁与合同测试已经实现，但在取得真实
厂商端点报告和 broker 侧消费凭证之前，多厂商互操作项继续保持未完成。
现场报告 schema v4 使用 `physicalDevice.connectionId` 将物理资产绑定到发布配置中的具体协议连接，
并包含逐连接及逐 MQTT sink 连续性验收证据。
一个包可以包含多协议，但门禁只统计该连接的协议，并核对 Runtime 指标中的连接协议与原始配置包一致；
其他物理资产或协议连接仍需独立活动，防止同一身份被错误复用为多种协议的厂商覆盖。

`field-mqtt-receipt` 已把 broker 回执生成收敛为可重复执行的 Runtime 工具。它读取与现场
Runtime 完全相同的发布包，复用生产 Topic 展开逻辑，覆盖全部启用的数据编排输出和 MQTT
sink；同时支持 MQTT 3.1.1/5.0、认证和 TLS。工具等待所有精确 Topic 的 SUBACK 后才计时，
过滤 retained、其他 edge/version 消息和 DUP 重投，并原子写出门禁所需的结构化回执。现场
仍需保留 VelaMQ 或其他 broker 的原生审计导出，作为独立真实性证据。

正式现场执行入口为 `field-campaign`。它用同一个发布包自动协调 MQTT 订阅就绪、生产 Runtime
长稳运行和发布收尾宽限，避免人工双终端启动顺序导致 24 小时证据失效；每次运行要求全新的
证据目录，并保留原始配置包、Runtime 报告、broker 回执、broker 原生审计、RocksDB 状态和带 SHA-256 绑定关系的
schema v3 活动清单。活动清单会在订阅、长稳运行和原生审计等待阶段原子更新；长稳窗口结束后可在有界
等待期内导出覆盖完整窗口的 broker 审计，避免导出时序竞争。只有 Runtime 已产生成功 MQTT 发布时才执行消费收尾宽限，
只有生成非空消费回执时才等待原生 broker 审计；启动失败或全程无进展时会立即固化失败清单，不再额外消耗收尾和审计等待时间。
活动程序在订阅就绪、长稳运行、消费收尾和原生审计等待阶段统一处理 `SIGINT/SIGTERM`；运维停止后会关闭当前会话、
保留已完成工件，并将原子清单标记为 `failed/interrupted`，不会遗留可被误判为仍在运行的现场证据目录。
正式现场主机可使用 `edgeops-field-campaign@.service` 按物理资产启动活动；受检包装器强制绝对证据路径、完整资产身份和
显式物理确认，统一应用 24 小时、失败率及最大进展间隔门限。服务禁用自动重启，任何重跑都必须使用新的证据目录并由操作员明确启动。
服务在正式启动前通过同一发布二进制执行无 I/O 预检，验证 Runtime 图、指定物理连接、MQTT 3.1.1/5.0 输出路由、密码环境变量、
自定义 CA、门限和新证据路径；预检不创建证据目录，也不连接设备或 Broker，失败不会污染或占用 24 小时活动窗口。
`field-campaign-plan` 进一步在部署实例前校验整站资产清单：绑定配置包摘要、物理连接和厂商/型号/序列号，检测跨活动的
client、RocksDB、证据目录及审计路径冲突，并证明计划资产满足部署策略中每种协议的厂商/型号数量要求。该报告只证明
现场计划可执行，最终门禁仍必须取得对应真实设备的 24 小时活动工件。
`field-campaign-status` 使用同一份计划持续汇总每个活动的 pending/running/passed/failed/invalid 状态，并对完成项重新验证
清单及四类工件摘要、计划身份和配置包绑定；日常观测不需要读取 Runtime 密码值，最终签核使用 `--require-complete`。
`field-interoperability-gate --campaign-dir` 会直接校验清单状态、文件路径和四个工件
摘要；`site` 发布配置通过 `EDGEOPS_FIELD_CAMPAIGN_PLAN` 接收已校验的整站计划，并使用版本化策略执行完整协议矩阵，避免
人工按顺序配对报告、配置、回执和审计。原生审计缺失、为空或摘要变化时门禁失败；该工件约束仍不能替代目标厂商设备本身。

长稳执行器把配置时长作为硬上限；任一已使用协议连接或 MQTT Sink 的成功计数超过允许静默期仍未前进时，
会提前生成带具体来源和观测间隔的失败报告，避免设备不可达或 Broker 无 PUBACK 时空跑完整个 24 小时窗口。
