# Containerized Modbus Device

This service is an independent Modbus TCP slave used for local field acceptance.
The Rust edge runtime communicates with it over a real TCP socket and standard
Modbus function codes.

## Register map

| Semantic point | Modbus address | Protocol offset | Type |
| --- | ---: | ---: | --- |
| Pump pressure | 40011-40012 | 10-11 | big-endian float32 |
| Pump flow | 40013-40014 | 12-13 | big-endian float32 |
| Pump running | 00001 | 0 | coil |
| Pump alarm | 00007 | 6 | coil |
| Temperature | 30001 | 0 | input register |

The pressure and flow values follow independent sine waves. The running and
alarm coils toggle at configurable intervals.

## Run

```bash
docker compose -f deploy/modbus-device/compose.yaml up -d --build
docker compose -f deploy/modbus-device/compose.yaml ps
```

The device is available to the host at `tcp://127.0.0.1:1502`. The container
health check reads holding registers with Modbus function code `03`.
