use cloud_control::{
    manufacturer_product_templates, ConfigValidator, ProductVersionStatus, OMRON_FINS_TEMPLATE_ID,
    SIEMENS_S7_TEMPLATE_ID,
};
use edge_core::{CommandGraphNodeKind, PointAccess, ProtocolType};

#[test]
fn manufacturer_templates_are_complete_deployable_products() {
    let templates = manufacturer_product_templates("factory-project");

    assert_eq!(templates.len(), 2);
    assert_eq!(templates[0].product.product_id, SIEMENS_S7_TEMPLATE_ID);
    assert_eq!(templates[1].product.product_id, OMRON_FINS_TEMPLATE_ID);

    for template in templates {
        assert_eq!(template.product.project_id, "factory-project");
        assert_eq!(template.version.status, ProductVersionStatus::Published);
        assert_eq!(
            template.version.point_set_ids,
            vec![template.point_set.point_set_id.clone()]
        );
        assert!(!template.point_set.points.is_empty());
        assert_eq!(template.version.protocol_connections.len(), 1);
        assert_eq!(template.version.collection_tasks.len(), 1);
        assert_eq!(template.version.data_configs.len(), 1);
        assert_eq!(template.version.command_flows.len(), 1);
        assert_eq!(template.version.mqtt_uplinks.len(), 1);
        assert!(template
            .point_set
            .points
            .iter()
            .any(|point| point.access == PointAccess::ReadWrite));

        template.version.protocol_connections[0]
            .validate()
            .expect("manufacturer connection contract must be valid");

        let package = template.materialize("edge-template-test");
        assert!(package.mqtt_uplinks[0]
            .client_id
            .starts_with("edge-template-test-"));
        assert!(ConfigValidator::validate_package(&package).is_empty());

        let write_node = package.command_flows[0]
            .nodes
            .iter()
            .find(|node| node.kind == CommandGraphNodeKind::PointWrite)
            .expect("command flow must include a point write node");
        let writable_point = package
            .point_mappings
            .iter()
            .find(|point| point.point_id == write_node.ref_id.as_deref().unwrap_or_default())
            .expect("point write node must reference a configured point");
        assert!(writable_point.access.is_writable());
    }
}

#[test]
fn manufacturer_templates_keep_protocol_specific_settings() {
    let templates = manufacturer_product_templates("factory-project");
    let s7 = templates
        .iter()
        .find(|template| template.product.product_id == SIEMENS_S7_TEMPLATE_ID)
        .expect("S7 template");
    let fins = templates
        .iter()
        .find(|template| template.product.product_id == OMRON_FINS_TEMPLATE_ID)
        .expect("FINS template");

    let s7_connection = &s7.version.protocol_connections[0];
    assert_eq!(s7_connection.protocol, ProtocolType::SiemensS7);
    assert!(s7_connection.siemens_s7.is_some());
    assert!(s7_connection.omron_fins.is_none());
    assert!(s7
        .point_set
        .points
        .iter()
        .all(|point| point.address.kind == "s7_address"));

    let fins_connection = &fins.version.protocol_connections[0];
    assert_eq!(fins_connection.protocol, ProtocolType::OmronFins);
    assert!(fins_connection.omron_fins.is_some());
    assert!(fins_connection.siemens_s7.is_none());
    assert!(fins
        .point_set
        .points
        .iter()
        .all(|point| point.address.kind == "fins_address"));
}
