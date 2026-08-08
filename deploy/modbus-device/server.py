#!/usr/bin/env python3

import argparse
import math
import os
import signal
import socket
import struct
import threading
import time

from pyModbusTCP.server import DataBank, ModbusServer


def env_float(name: str, default: float) -> float:
    return float(os.getenv(name, str(default)))


def env_int(name: str, default: int) -> int:
    return int(os.getenv(name, str(default)))


def float32_words(value: float) -> list[int]:
    return list(struct.unpack(">HH", struct.pack(">f", value)))


def read_exact(sock: socket.socket, size: int) -> bytes:
    data = bytearray()
    while len(data) < size:
        chunk = sock.recv(size - len(data))
        if not chunk:
            raise ConnectionError("Modbus server closed the connection")
        data.extend(chunk)
    return bytes(data)


def check_server(host: str, port: int, unit_id: int) -> None:
    transaction_id = 1
    request = struct.pack(
        ">HHHBBHH",
        transaction_id,
        0,
        6,
        unit_id,
        3,
        10,
        2,
    )
    with socket.create_connection((host, port), timeout=2) as sock:
        sock.sendall(request)
        header = read_exact(sock, 7)
        response_transaction, protocol_id, length, response_unit = struct.unpack(
            ">HHHB", header
        )
        body = read_exact(sock, length - 1)

    if response_transaction != transaction_id or protocol_id != 0:
        raise RuntimeError("invalid Modbus TCP response header")
    if response_unit != unit_id or body[:2] != bytes((3, 4)):
        raise RuntimeError("invalid Modbus TCP holding-register response")


def run_server() -> None:
    host = os.getenv("MODBUS_BIND_HOST", "0.0.0.0")
    port = env_int("MODBUS_PORT", 502)
    unit_id = env_int("MODBUS_UNIT_ID", 1)
    update_interval = env_float("UPDATE_INTERVAL_SECONDS", 1.0)

    pressure_center = env_float("PRESSURE_CENTER", 2.4)
    pressure_amplitude = env_float("PRESSURE_AMPLITUDE", 0.18)
    pressure_period = env_float("PRESSURE_PERIOD_SECONDS", 20.0)

    flow_center = env_float("FLOW_CENTER", 2.6)
    flow_amplitude = env_float("FLOW_AMPLITUDE", 0.12)
    flow_period = env_float("FLOW_PERIOD_SECONDS", 16.0)

    running_period = env_float("RUNNING_PERIOD_SECONDS", 15.0)
    alarm_period = env_float("ALARM_PERIOD_SECONDS", 10.0)

    data_bank = DataBank()
    data_bank.set_holding_registers(10, float32_words(pressure_center))
    data_bank.set_holding_registers(12, float32_words(flow_center))
    data_bank.set_coils(0, [False])
    data_bank.set_coils(6, [False])
    data_bank.set_input_registers(0, [36])

    server = ModbusServer(
        host=host,
        port=port,
        no_block=True,
        data_bank=data_bank,
    )
    stop_event = threading.Event()

    def stop_server(_signum: int, _frame: object) -> None:
        stop_event.set()

    signal.signal(signal.SIGINT, stop_server)
    signal.signal(signal.SIGTERM, stop_server)

    server.start()
    print(
        f"VelaEdge Modbus device listening on {host}:{port}, unit={unit_id}",
        flush=True,
    )

    started_at = time.monotonic()
    try:
        while not stop_event.wait(update_interval):
            elapsed = time.monotonic() - started_at
            pressure = pressure_center + pressure_amplitude * math.sin(
                elapsed * 2 * math.pi / pressure_period
            )
            flow = flow_center + flow_amplitude * math.sin(
                elapsed * 2 * math.pi / flow_period
            )
            running = int(elapsed / running_period) % 2 == 1
            alarm = int(elapsed / alarm_period) % 2 == 1

            data_bank.set_holding_registers(10, float32_words(pressure))
            data_bank.set_holding_registers(12, float32_words(flow))
            data_bank.set_coils(0, [running])
            data_bank.set_coils(6, [alarm])
    finally:
        server.stop()
        print("VelaEdge Modbus device stopped", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Containerized Modbus TCP device")
    parser.add_argument("--healthcheck", action="store_true")
    args = parser.parse_args()

    if args.healthcheck:
        check_server(
            os.getenv("MODBUS_HEALTH_HOST", "127.0.0.1"),
            env_int("MODBUS_PORT", 502),
            env_int("MODBUS_UNIT_ID", 1),
        )
        return

    run_server()


if __name__ == "__main__":
    main()
