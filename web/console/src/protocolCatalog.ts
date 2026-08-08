import type { RuntimeProtocolDescriptor } from './api/types';

export const fallbackProtocolCatalog: RuntimeProtocolDescriptor[] = [
  descriptor('ModbusTcp', 'modbus-tcp', 'Modbus TCP', 'tcp', true, false),
  descriptor('ModbusRtu', 'modbus-rtu', 'Modbus RTU', 'serial', true, true),
  descriptor('Dlt645', 'dlt645-2007', 'DL/T 645-2007', 'serial', false, false),
  descriptor('Iec101', 'iec60870-5-101-unbalanced', 'IEC 60870-5-101', 'serial', true, false),
  descriptor('Iec104', 'iec60870-5-104-client', 'IEC 60870-5-104', 'tcp', true, false),
  descriptor('CustomSerial', 'custom-serial-frame-dsl-v2', '自定义串口帧 DSL', 'serial', false, false),
  descriptor('OpcUa', 'opc-ua-client', 'OPC UA', 'tcp', true, true),
  descriptor('BacnetIp', 'bacnet-ip', 'BACnet/IP', 'udp', true, false),
  descriptor('SiemensS7', 'siemens-s7', 'Siemens S7', 'tcp', true, false),
  descriptor('OmronFins', 'omron-fins', 'Omron FINS', 'tcp_udp', true, false),
  descriptor('Simulated', 'simulated', '模拟协议', 'internal', true, false, 'laboratory'),
];

export function configuredProtocolCatalog(
  catalog: RuntimeProtocolDescriptor[] | undefined,
): RuntimeProtocolDescriptor[] {
  return catalog?.length ? catalog.filter((item) => item.maturity !== 'planned') : fallbackProtocolCatalog;
}

export function protocolOptionsFromCatalog(
  catalog: RuntimeProtocolDescriptor[] | undefined,
): Array<readonly [string, string]> {
  return configuredProtocolCatalog(catalog).map((item) => [item.protocolType, item.displayName]);
}

function descriptor(
  protocolType: string,
  capabilityId: string,
  displayName: string,
  transport: RuntimeProtocolDescriptor['transport'],
  commandWrite: boolean,
  automaticDiscovery: boolean,
  maturity: RuntimeProtocolDescriptor['maturity'] = 'deployment_candidate',
): RuntimeProtocolDescriptor {
  return {
    automaticDiscovery,
    capabilityId,
    commandWrite,
    displayName,
    maturity,
    protocolType,
    telemetryRead: true,
    transport,
  };
}
