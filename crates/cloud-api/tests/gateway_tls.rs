use cloud_api::gateway::EdgeGatewayTlsConfig;

#[test]
fn gateway_tls_config_rejects_invalid_certificate_material() {
    let error = match EdgeGatewayTlsConfig::from_pem(
        "not a server cert",
        "not a server key",
        "not a client ca cert",
    ) {
        Ok(_) => panic!("invalid gateway certificate material should fail"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("server certificate"));
}
