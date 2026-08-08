# 指令编排设计

## 定位

指令编排负责下行控制链路，与采集编排独立：

- 采集编排：工业协议读取 -> 计算 -> MQTT 上报。
- 指令编排：MQTT 订阅 -> 解析与策略 -> 工业协议写入 -> MQTT 回执。

两者共享产品、点位集、协议连接和画布基础组件，但分别保存、校验、发布、
授权、审计和监控。

## 配置模型

一条指令流包含：

1. MQTT 输入：连接、订阅 Topic、QoS、共享订阅选项。
2. 消息解析：JSON Path 到命令 ID、目标设备、点位和值的映射。
3. 路由/条件：按命令、设备、值或业务字段形成多分支。
4. 安全策略：类型、范围、速率、超时、幂等、确认令牌和角色限制。
5. 写点位：绑定一个显式可写点位，并由协议驱动决定 Modbus 功能码。
6. 回执输出：发布 accepted/executing/succeeded/failed 状态和写后读结果。

## 点位访问控制

点位访问权限是配置合同的一部分：

- `read_only`：只出现在采集编排。
- `read_write`：可同时出现在采集与指令编排。
- `write_only`：只出现在指令编排。

默认值为 `read_only`。即使云端错误下发，Runtime 仍会拒绝只读点位写入；Modbus
输入寄存器和离散输入属于协议级只读区，不能通过配置放开。

写点位节点必须在产品配置中固定绑定 `pointId`。MQTT 消息只提供该点位的目标值，
不能通过载荷临时指定任意点位，从而避免绕过画布和权限模型扩大写入范围。

写节点默认使用 `verification=response`，校验 Modbus 写响应。关键控制点可配置
`verification=readback`，Runtime 会在写入后读取同一点位并比较；浮点点位默认容差
为 `1e-6`，也可通过 `readback_tolerance` 显式设置。验证模式、期望值、实际读回值
和最终结果都会进入 RocksDB 指令审计。

## 安全门配置

安全门节点支持以下参数：

| 参数 | 含义 | 约束 |
| --- | --- | --- |
| `require_confirmation` | 是否要求确认令牌 | 布尔值；启用后载荷必须包含非空 `confirmationToken` |
| `source_path` | 指令来源字段路径 | 默认 `requestedBy` |
| `allowed_sources` | 允许的逻辑来源 | 非空字符串数组；来源缺失或不在列表中时拒绝写入 |
| `max_commands` | 窗口内最多执行次数 | 必须与 `window_ms` 同时配置且大于 0 |
| `window_ms` | 滚动限流窗口 | 必须与 `max_commands` 同时配置且大于 0 |

`requestedBy` 是业务载荷中的逻辑来源，不是密码学身份。生产环境必须同时使用
MQTT Broker ACL、独立客户端凭证和 Topic 发布授权，确保只有可信发布者能够写入
指令 Topic；Runtime 的来源白名单是第二层业务授权，不能替代 Broker 认证。

## MQTT 命令格式

```json
{
  "commandId": "019f-command-id",
  "deviceId": "pump-1",
  "pointId": "pressure_setpoint",
  "value": 12.5,
  "requestedBy": "scada",
  "issuedAt": "2026-07-31T10:00:00Z",
  "expiresAt": "2026-07-31T10:00:10Z",
  "confirmationToken": null
}
```

多点指令也可以使用业务自定义的嵌套 JSON。指令编排为每个固定可写点位配置
`value_path`，例如 `payload.control.speed` 和 `payload.control.start`：

```json
{
  "commandId": "019f-command-id",
  "requestedBy": "scada",
  "payload": {
    "control": {
      "speed": 1450,
      "start": true
    }
  }
}
```

Topic 与字段路径均可配置，但字段路径只能提供目标值；写入点位仍由产品指令流静态绑定。

默认回执 Topic 为原 Topic 加 `/reply/{commandId}`，也可在编排中独立配置。

## Runtime 执行顺序

1. MQTT 长连接订阅并校验包大小。
2. 检查命令 ID 幂等记录、过期时间和来源权限。
3. 解析目标点位并确认访问权限。
4. 执行类型、范围、速率和人工确认策略。
5. 调用 `ProtocolCommandAdapter`。
6. 可选写后读验证。
7. RocksDB 记录命令、结果和审计信息。
8. 发布 MQTT 回执并更新运行指标。

## Modbus 写入映射

| 点位区 | 单值 | 多值 | 说明 |
| --- | --- | --- | --- |
| Coil | FC05 | FC15（规划） | 布尔量；首版指令节点执行单点写入 |
| Holding Register | FC06 | FC16 | 整数、布尔或多寄存器浮点 |
| Discrete Input | 不支持 | 不支持 | 物理只读 |
| Input Register | 不支持 | 不支持 | 物理只读 |

## IEC 104 控制映射

IEC 104 可写点位必须在产品点位集中固定控制 ASDU，不能由 MQTT 载荷动态覆盖：

| 语义类型 | 控制 ASDU | 允许值 | 执行规则 |
| --- | --- | --- | --- |
| 布尔 | `C_SC_NA_1` | `false` / `true` | 可选 SBO |
| 整数 | `C_DC_NA_1` | `1=OFF` / `2=ON` | 拒绝保留值，可选 SBO |
| 浮点 | `C_SE_NC_1` | 有限 `f32` | 可选 SBO |

Runtime 会在建立连接前完成类型和值域检查。启用 SBO 时先发送选择命令并等待同一
IOA 的肯定激活确认，再发送执行命令；未启用时直接执行。超时、否定确认、服务端错误
或断链都会使本次指令失败、清理会话，并阻止影子更新与成功回执。

## 验收标准

- 只读点位无法进入写点位节点，篡改配置后 Runtime 仍拒绝执行。
- 同一个命令 ID 重复投递不会重复写设备。
- 支持一条 MQTT 输入分支到多个设备写入和多个回执 Topic。
- 每次执行都具备请求、策略决策、协议响应、耗时和最终结果审计。
- 断网后不执行过期命令；恢复连接后回执可补发。

## 当前实现状态

- 已完成：`read_write`/`write` 点位筛选，Cloud 与 Runtime 双重校验，Modbus
  TCP/RTU 单线圈与保持寄存器写入，范围/类型/过期/确认令牌预检，多分支写入与回执。
- 已完成：MQTT 3.1.1/5.0 指令主题订阅、`+/#` 路由、一条消息匹配多条指令流、
  重连后自动重订阅，以及执行结果发布入口。
- 已完成：守护进程按配置版本热切换指令服务，命令 ID 与载荷摘要幂等校验、
  RocksDB 执行审计、重复命令回执重放和 MQTT 出站箱补发。指令订阅、回执发布与
  遥测发布使用独立 Client ID，避免 Broker 会话互相替换。
- 已完成：写节点可选 Modbus TCP/RTU 写后读验证、数值容差比较、读回值回执与
  持久化审计，验证失败会终止当前指令流。
- 已完成：IEC 104 `C_SC_NA_1`、`C_DC_NA_1`、`C_SE_NC_1` 写入，可选 SBO、
  肯定激活确认、会话失败重建、影子/指标/回执集成和真实 TCP 报文测试。
- 已完成：安全门来源白名单、来源审计和滚动窗口速率限制；策略错误会在协议写入前
  失败，并生成标准 MQTT 回执和 RocksDB 审计。Cloud 发布前会校验来源和限流参数。
- 已完成：生产 MQTT 指令服务将安全门滚动窗口原子持久化到 RocksDB；Runtime 或
  配置服务重启后仍延续窗口，多安全门只有全部允许时才一次性消耗限额。无存储的
  嵌入式执行 API 使用进程内窗口，不作为生产守护进程入口。
- 已完成：产品配置中的指令编排页面，可配置订阅/回执 Topic、安全策略、多个可写点位、
  每个点位的 JSON 字段路径及写入验证模式。
- 待完成：连接真实 MQTT Broker 与独立工业设备的持续现场验收和故障注入。

可执行一次真实链路验收：

```bash
VELAEDGE_COMMAND_MQTT_HOST=127.0.0.1 \
VELAEDGE_COMMAND_MQTT_PORT=1883 \
VELAEDGE_COMMAND_MODBUS_ENDPOINT=127.0.0.1:1502 \
VELAEDGE_COMMAND_MQTT_BROKER_LABEL=VelaMQ \
VELAEDGE_COMMAND_MODBUS_CONTAINER=modbus-device-modbus-device-1 \
scripts/run-mqtt-modbus-command-acceptance.sh
```

脚本会预检 Broker 与设备端口，执行 MQTT 指令订阅、JSON 取值、Modbus FC06 写入与读回、
MQTT 回执全链路，并保留 `report.json` 和测试日志。报告明确标记容器设备不是物理现场设备，
不能替代厂商互操作或 24 小时验收。
