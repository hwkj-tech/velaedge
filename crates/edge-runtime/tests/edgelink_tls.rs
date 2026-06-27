use edge_runtime::EdgeLinkClientTlsConfig;

#[test]
fn runtime_tls_config_rejects_invalid_certificate_material() {
    let config = EdgeLinkClientTlsConfig {
        ca_cert_pem: "not a ca cert".to_string(),
        client_cert_pem: "not a client cert".to_string(),
        client_key_pem: "not a client key".to_string(),
        server_name: "localhost".to_string(),
    };

    let error = match config.build_connector() {
        Ok(_) => panic!("invalid runtime certificate material should fail"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("CA certificate"));
}
