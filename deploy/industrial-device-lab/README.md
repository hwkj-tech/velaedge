# Industrial protocol device lab

This lab starts four stateful protocol devices for VelaEdge integration tests. They
serve actual Siemens S7, Omron FINS, IEC 104 and BACnet/IP frames rather than returning
HTTP fixtures.

## Start

```bash
docker compose -f deploy/industrial-device-lab/compose.yaml up -d --build --wait
```

Default host endpoints:

| Device | Endpoint | Dynamic data | Writable command |
| --- | --- | --- | --- |
| Siemens S7 | `127.0.0.1:11102` | `DB1.REAL0`, `DB1.DINT6` | `DB1.DBX10.0` |
| Omron FINS TCP/UDP | `127.0.0.1:19600` | `D100`, `D102` | `CIO0.1` |
| IEC 104 | `127.0.0.1:12404` | `1:1001`, `1:1002` | `1:1201`, `1:1202`, `1:1203` |
| BACnet/IP | `127.0.0.1:14780/udp` | device 42, `analog-input:1` | device 42, `analog-value:7` |

The S7 command at `DB1.DBX10.0` controls `DB1.DBX4.0` and simulated speed. The
FINS command at `CIO0.1` controls `CIO0.0`. The IEC 104 RTU provides STARTDT, general interrogation,
spontaneous telemetry and `C_SC_NA_1`/`C_DC_NA_1`/`C_SE_NC_1` control with SBO
confirmation. The BACnet/IP device responds to directed Who-Is, serves
ReadPropertyMultiple, emits unconfirmed COV notifications, and accepts priority-aware
WriteProperty requests. These addresses match the VelaEdge acceptance packages.

Override ports when needed:

```bash
S7_HOST_PORT=21102 FINS_HOST_PORT=29600 IEC104_HOST_PORT=22404 BACNET_HOST_PORT=24780 \
  docker compose -f deploy/industrial-device-lab/compose.yaml up -d --build --wait
```

Run production-adapter acceptance against the containers:

```bash
scripts/run-container-protocol-device-acceptance.sh
```

This is repeatable protocol-level evidence. It does not replace physical PLC,
firmware-compatibility, electrical-noise, or 24-hour field endurance evidence.
