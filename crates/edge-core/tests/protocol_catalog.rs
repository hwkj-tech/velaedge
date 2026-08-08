use edge_core::{
    ProtocolType, RuntimeProtocolCatalog, RuntimeProtocolMaturity, RuntimeProtocolTransport,
};

#[test]
fn catalog_covers_every_runtime_protocol_and_declares_real_capabilities() {
    let catalog = RuntimeProtocolCatalog::all();
    assert_eq!(catalog.len(), 11);

    let protocols = [
        ProtocolType::Simulated,
        ProtocolType::ModbusTcp,
        ProtocolType::ModbusRtu,
        ProtocolType::Dlt645,
        ProtocolType::Iec101,
        ProtocolType::Iec104,
        ProtocolType::CustomSerial,
        ProtocolType::OpcUa,
        ProtocolType::BacnetIp,
        ProtocolType::SiemensS7,
        ProtocolType::OmronFins,
    ];
    for protocol in protocols {
        assert_eq!(
            catalog
                .iter()
                .filter(|descriptor| descriptor.protocol_type == protocol)
                .count(),
            1,
            "{protocol:?} must have exactly one capability descriptor"
        );
    }

    let opc_ua = RuntimeProtocolCatalog::descriptor(ProtocolType::OpcUa);
    assert!(opc_ua.automatic_discovery);
    assert_eq!(opc_ua.transport, RuntimeProtocolTransport::Tcp);

    let bacnet = RuntimeProtocolCatalog::descriptor(ProtocolType::BacnetIp);
    assert!(!bacnet.automatic_discovery);
    assert!(bacnet.command_write);

    let simulated = RuntimeProtocolCatalog::descriptor(ProtocolType::Simulated);
    assert_eq!(simulated.maturity, RuntimeProtocolMaturity::Laboratory);
    assert_eq!(RuntimeProtocolCatalog::executable().len(), catalog.len());
}
