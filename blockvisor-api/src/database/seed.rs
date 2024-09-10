//! Seed new test databases with data for integration testing.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::auth::rbac::access::tests::view_authz;
use crate::auth::rbac::{BlockjoyRole, OrgRole, ViewRole};
use crate::auth::resource::{NodeId, OrgId, ResourceType, UserId};
use crate::model::host::{Host, NewHost, ScheduleType};
use crate::model::image::config::ConfigType;
use crate::model::image::{Config, Image, ImageId, NewConfig, NodeConfig};
use crate::model::ip_address::CreateIpAddress;
use crate::model::node::{Node, NodeState, ResourceAffinity};
use crate::model::protocol::version::{ProtocolVersion, VersionId};
use crate::model::protocol::{Protocol, ProtocolId};
use crate::model::rbac::RbacUser;
use crate::model::schema::{images, nodes, orgs, protocol_versions, protocols};
use crate::model::user::NewUser;
use crate::model::{IpAddress, Org, Region, User};
use crate::util::sql::{IpNetwork, Tag, Tags};

use super::Conn;

pub const SUPER_EMAIL: &str = "super@user.com";
pub const ADMIN_EMAIL: &str = "admin@org.com";
pub const MEMBER_EMAIL: &str = "member@org.com";
pub const UNKNOWN_EMAIL: &str = "unknown@other.com";
pub const UNCONFIRMED_EMAIL: &str = "unconfirmed@org.com";
pub const LOGIN_PASSWORD: &str = "hunter2";

pub const PROTOCOL_ID: &str = "ab5d8cfc-77b1-4265-9fee-ba71ba9de092";
pub const PROTOCOL_KEY: &str = "ethereum";
pub const PROTOCOL_VISIBILITY: &str = "public";
pub const PROTOCOL_VERSION_ID: &str = "a69e7195-8a78-4e3a-a79e-4ac89edf1d68";
pub const VARIANT_KEY: &str = "sepolia";
pub const SEMANTIC_VERSION: &str = "1.2.3";
pub const SKU_CODE: &str = "ETH-TN";

pub const ORG_ID: &str = "08dede71-b97d-47c1-a91d-6ba0997b3cdd";
pub const ORG_PROTOCOL_ID: &str = "9331899f-3b13-4d03-ade5-5580ca93ed01";
pub const ORG_PROTOCOL_KEY: &str = "solana";
pub const ORG_VARIANT_KEY: &str = "testnet";
pub const ORG_PROTOCOL_VERSION_ID: &str = "77b22a4a-a656-4d02-ab52-b56806047a56";
pub const ORG_SEMANTIC_VERSION: &str = "1.2.4";
pub const ORG_IMAGE_ID: &str = "5537849e-9003-46c0-8129-b465ffbb06f2";

pub const IMAGE_ID: &str = "fb56b151-443b-491a-a2a5-50fc12343a91";
pub const MEMORY_BYTES: i64 = 1024_i64.pow(3);
pub const DISK_BYTES: i64 = 1024_i64.pow(4);
pub const DEFAULT_FIREWALL_IN: &str = "drop";
pub const DEFAULT_FIREWALL_OUT: &str = "allow";

pub const IMAGE_PROPERTY_ID_1: &str = "f9728f8b-a4c8-438c-b9c9-5e5e63aa0fe0";
pub const IMAGE_PROPERTY_ID_2: &str = "8b1281a8-4dfa-4c57-a948-80c4f7e7a287";
pub const NETWORK_KEY: &str = "network";
pub const MORE_RESOURCES_KEY: &str = "more-resources";

pub const ARCHIVE_ID_1: &str = "d4c6a35e-2804-4feb-a052-234e91d7ac8b";
pub const ARCHIVE_ID_2: &str = "e54dab10-5c62-4778-8aba-dc5162b48025";
pub const STORE_ID_1: &str = "store-1";
pub const STORE_ID_2: &str = "store-2";

pub const HOST_1: &str = "host-1";
pub const HOST_2: &str = "host-2";

pub const NODE_ID: &str = "cdbbc736-f399-42ab-86cf-617ce983011d";
pub const NODE_NAME: &str = "node-1";

pub const IP_RANGE: [&str; 10] = [
    "127.0.0.1",
    "127.0.0.2",
    "127.0.0.3",
    "127.0.0.4",
    "127.0.0.5",
    "127.0.0.6",
    "127.0.0.7",
    "127.0.0.8",
    "127.0.0.9",
    "127.0.0.10",
];

pub struct Seed {
    pub protocol: Protocol,
    pub version: ProtocolVersion,
    pub image: Image,
    pub admin: User,
    pub member: User,
    pub org: Org,
    pub host1: Host,
    pub host2: Host,
    pub node: Node,
    pub config: Config,
    pub region: Region,
}

impl Seed {
    pub async fn new(conn: &mut Conn<'_>) -> Self {
        setup_rbac(conn).await;

        let org = create_orgs(conn).await;
        let (admin, member) = create_users(org.id, conn).await;
        let region = create_region(conn).await;
        let (host1, host2) = create_hosts(admin.id, org.id, &region, conn).await;
        let (ip_address, ip_gateway) = create_ip_range(&host1, conn).await;
        let (protocol, version, image) = create_image(conn).await;
        let (node, config) =
            create_node(&image, &host1, &protocol, ip_address, ip_gateway, conn).await;

        Seed {
            protocol,
            version,
            image,
            admin,
            member,
            org,
            host1,
            host2,
            node,
            config,
            region,
        }
    }
}

async fn create_image(conn: &mut Conn<'_>) -> (Protocol, ProtocolVersion, Image) {
    let queries = [
        format!("INSERT INTO protocols (id, org_id, key, name, visibility)
            VALUES ('{PROTOCOL_ID}', null, '{PROTOCOL_KEY}', '{PROTOCOL_KEY}', '{PROTOCOL_VISIBILITY}');"),
        format!("INSERT INTO protocols (id, org_id, key, name, visibility)
            VALUES ('{ORG_PROTOCOL_ID}', '{ORG_ID}', '{ORG_PROTOCOL_KEY}', '{ORG_PROTOCOL_KEY}', '{PROTOCOL_VISIBILITY}');"),
        format!("INSERT INTO protocol_versions (id, org_id, protocol_id, protocol_key, variant_key, semantic_version, sku_code, visibility)
            VALUES ('{PROTOCOL_VERSION_ID}', null, '{PROTOCOL_ID}', '{PROTOCOL_KEY}', '{VARIANT_KEY}', '{SEMANTIC_VERSION}', '{SKU_CODE}', '{PROTOCOL_VISIBILITY}');"),
        format!("INSERT INTO protocol_versions (id, org_id, protocol_id, protocol_key, variant_key, semantic_version, sku_code, visibility)
            VALUES ('{ORG_PROTOCOL_VERSION_ID}', '{ORG_ID}', '{ORG_PROTOCOL_ID}', '{ORG_PROTOCOL_KEY}', '{VARIANT_KEY}', '{ORG_SEMANTIC_VERSION}', '{SKU_CODE}', '{PROTOCOL_VISIBILITY}');"),
        format!("INSERT INTO images (id, org_id, protocol_version_id, image_uri, build_version, min_cpu_cores, min_memory_bytes, min_disk_bytes, default_firewall_in, default_firewall_out, visibility)
            VALUES ('{IMAGE_ID}', null, '{PROTOCOL_VERSION_ID}', 'docker:TODO', 1, 1, {MEMORY_BYTES}, {DISK_BYTES}, '{DEFAULT_FIREWALL_IN}', '{DEFAULT_FIREWALL_OUT}', '{PROTOCOL_VISIBILITY}');"),
        format!("INSERT INTO images (id, org_id, protocol_version_id, image_uri, build_version, min_cpu_cores, min_memory_bytes, min_disk_bytes, default_firewall_in, default_firewall_out, visibility)
            VALUES ('{ORG_IMAGE_ID}', '{ORG_ID}', '{ORG_PROTOCOL_VERSION_ID}', 'docker:TODO', 1, 1, {MEMORY_BYTES}, {DISK_BYTES}, '{DEFAULT_FIREWALL_IN}', '{DEFAULT_FIREWALL_OUT}', '{PROTOCOL_VISIBILITY}');"),
        format!("INSERT INTO image_properties (id, image_id, key, new_archive, default_value, ui_type)
            VALUES ('{IMAGE_PROPERTY_ID_1}', '{IMAGE_ID}', '{NETWORK_KEY}', false, 'testnet', 'enum');"),
        format!("INSERT INTO image_properties (id, image_id, key, new_archive, default_value, ui_type, add_cpu_cores, add_memory_bytes, add_disk_bytes)
            VALUES ('{IMAGE_PROPERTY_ID_2}', '{IMAGE_ID}', '{MORE_RESOURCES_KEY}', true, 'resources', 'switch', 1, {MEMORY_BYTES}, {DISK_BYTES});"),
        format!("INSERT INTO archives (id, org_id, image_id, store_id, image_property_ids)
            VALUES ('{ARCHIVE_ID_1}', null, '{IMAGE_ID}', '{STORE_ID_1}', array[]::uuid[]);"),
        format!("INSERT INTO archives (id, org_id, image_id, store_id, image_property_ids)
            VALUES ('{ARCHIVE_ID_2}', null, '{IMAGE_ID}', '{STORE_ID_2}', '{{ {IMAGE_PROPERTY_ID_2} }}');"),
    ];

    for query in queries {
        diesel::sql_query(query).execute(conn).await.unwrap();
    }

    let protocol_id: ProtocolId = PROTOCOL_ID.parse().unwrap();
    let protocol = protocols::table
        .find(protocol_id)
        .get_result(conn)
        .await
        .unwrap();

    let version_id: VersionId = PROTOCOL_VERSION_ID.parse().unwrap();
    let version = protocol_versions::table
        .find(version_id)
        .get_result(conn)
        .await
        .unwrap();

    let image_id: ImageId = IMAGE_ID.parse().unwrap();
    let image = images::table.find(image_id).get_result(conn).await.unwrap();

    (protocol, version, image)
}

async fn create_orgs(conn: &mut Conn<'_>) -> Org {
    let org_id: OrgId = ORG_ID.parse().unwrap();

    diesel::insert_into(orgs::table)
        .values((
            orgs::id.eq(org_id),
            orgs::name.eq("the blockboys"),
            orgs::is_personal.eq(false),
            orgs::stripe_customer_id.eq("testing testing, is thing thing even on?"),
        ))
        .execute(conn)
        .await
        .unwrap();

    Org::by_id(org_id, conn).await.unwrap()
}

async fn create_users(org_id: OrgId, conn: &mut Conn<'_>) -> (User, User) {
    let new_user = |email, first, last| NewUser::new(email, first, last, LOGIN_PASSWORD).unwrap();
    let super_user = new_user(SUPER_EMAIL, "Super", "User")
        .create(conn)
        .await
        .unwrap();
    let admin = new_user(ADMIN_EMAIL, "Org", "Admin")
        .create(conn)
        .await
        .unwrap();
    let member = new_user(MEMBER_EMAIL, "Bog", "Standard")
        .create(conn)
        .await
        .unwrap();
    let unknown = new_user(UNKNOWN_EMAIL, "Sus", "Guy")
        .create(conn)
        .await
        .unwrap();
    let _ = new_user(UNCONFIRMED_EMAIL, "Not", "Yet")
        .create(conn)
        .await
        .unwrap();

    User::confirm(super_user.id, conn).await.unwrap();
    User::confirm(admin.id, conn).await.unwrap();
    User::confirm(member.id, conn).await.unwrap();
    User::confirm(unknown.id, conn).await.unwrap();

    Org::add_user(admin.id, org_id, OrgRole::Admin, conn)
        .await
        .unwrap();
    Org::add_user(member.id, org_id, OrgRole::Member, conn)
        .await
        .unwrap();

    RbacUser::link_role(super_user.id, org_id, BlockjoyRole::Admin, conn)
        .await
        .unwrap();
    RbacUser::link_role(admin.id, org_id, ViewRole::DeveloperPreview, conn)
        .await
        .unwrap();

    let admin = User::by_id(admin.id, conn).await.unwrap();
    let member = User::by_id(member.id, conn).await.unwrap();

    (admin, member)
}

async fn create_region(conn: &mut Conn<'_>) -> Region {
    Region::get_or_create("moneyland", Some("MOLA"), conn)
        .await
        .unwrap()
}

async fn create_hosts(
    created_by_id: UserId,
    org_id: OrgId,
    region: &Region,
    conn: &mut Conn<'_>,
) -> (Host, Host) {
    let bv_version = "0.1.0".parse().unwrap();

    let host1 = NewHost {
        org_id: None,
        region_id: Some(region.id),
        network_name: HOST_1,
        display_name: None,
        schedule_type: ScheduleType::Automatic,
        os: "LuukOS",
        os_version: "1",
        bv_version: &bv_version,
        ip_address: "192.168.1.1".parse().unwrap(),
        ip_gateway: "192.168.1.1".parse().unwrap(),
        cpu_cores: 100,
        memory_bytes: 100 * MEMORY_BYTES,
        disk_bytes: 100 * DISK_BYTES,
        tags: Tags(vec![Tag::new(PROTOCOL_KEY.to_string()).unwrap()]),
        created_by_type: ResourceType::User,
        created_by_id: created_by_id.into(),
    };
    let host1 = host1
        .create(&["192.168.1.2".parse().unwrap()], conn)
        .await
        .unwrap();

    let host2 = NewHost {
        org_id: Some(org_id),
        region_id: Some(region.id),
        network_name: HOST_2,
        display_name: None,
        schedule_type: ScheduleType::Automatic,
        os: "TempleOS",
        os_version: "2",
        bv_version: &bv_version,
        ip_address: "192.168.2.1".parse().unwrap(),
        ip_gateway: "192.168.2.1".parse().unwrap(),
        cpu_cores: 1,
        memory_bytes: MEMORY_BYTES,
        disk_bytes: DISK_BYTES,
        tags: Tags(vec![Tag::new(PROTOCOL_KEY.to_string()).unwrap()]),
        created_by_type: ResourceType::User,
        created_by_id: created_by_id.into(),
    };
    let host2 = host2
        .create(&["192.168.2.1".parse().unwrap()], conn)
        .await
        .unwrap();

    (host1, host2)
}

async fn create_ip_range(host: &Host, conn: &mut Conn<'_>) -> (IpNetwork, IpNetwork) {
    let ips = IP_RANGE
        .iter()
        .map(|ip| CreateIpAddress::new(ip.parse().unwrap(), host.id))
        .collect();
    CreateIpAddress::bulk_create(ips, conn).await.unwrap();

    let ip_address = IpAddress::next_for_host(host.id, conn)
        .await
        .unwrap()
        .unwrap()
        .ip;

    (ip_address, host.ip_gateway)
}

async fn create_node(
    image: &Image,
    host: &Host,
    protocol: &Protocol,
    ip_address: IpNetwork,
    ip_gateway: IpNetwork,
    conn: &mut Conn<'_>,
) -> (Node, Config) {
    let org_id: OrgId = ORG_ID.parse().unwrap();
    let node_id: NodeId = NODE_ID.parse().unwrap();
    let image_id: ImageId = IMAGE_ID.parse().unwrap();
    let version_id: VersionId = PROTOCOL_VERSION_ID.parse().unwrap();

    let node_config = NodeConfig::new(image.clone(), Some(org_id), vec![], vec![], conn)
        .await
        .unwrap();
    let new_config = NewConfig {
        image_id: IMAGE_ID.parse().unwrap(),
        archive_id: ARCHIVE_ID_1.parse().unwrap(),
        config_type: ConfigType::Node,
        config: node_config.into(),
    };
    let config = new_config.create(&view_authz(node_id), conn).await.unwrap();

    let node = diesel::insert_into(nodes::table)
        .values((
            nodes::id.eq(node_id),
            nodes::node_name.eq(NODE_NAME),
            nodes::display_name.eq(NODE_NAME),
            nodes::org_id.eq(org_id),
            nodes::host_id.eq(host.id),
            nodes::image_id.eq(image_id),
            nodes::config_id.eq(config.id),
            nodes::protocol_id.eq(protocol.id),
            nodes::protocol_version_id.eq(version_id),
            nodes::semantic_version.eq(SEMANTIC_VERSION),
            nodes::auto_upgrade.eq(true),
            nodes::node_state.eq(NodeState::Running),
            nodes::ip_address.eq(ip_address),
            nodes::ip_gateway.eq(ip_gateway),
            nodes::dns_id.eq("dns.id"),
            nodes::dns_name.eq(NODE_NAME),
            nodes::cpu_cores.eq(1),
            nodes::memory_bytes.eq(MEMORY_BYTES),
            nodes::disk_bytes.eq(DISK_BYTES),
            nodes::scheduler_resource.eq(ResourceAffinity::LeastResources),
            nodes::created_by_type.eq(ResourceType::Org),
            nodes::created_by_id.eq(org_id),
        ))
        .get_result(conn)
        .await
        .unwrap();

    (node, config)
}

async fn setup_rbac(conn: &mut Conn<'_>) {
    super::create_roles_and_perms(conn).await.unwrap();

    let query = "
        insert into role_permissions (role, permission)
        values
        -- blockjoy-admin --
        ('blockjoy-admin', 'auth-admin-list-permissions'),
        ('blockjoy-admin', 'billing-exempt'),
        ('blockjoy-admin', 'command-admin-list'),
        ('blockjoy-admin', 'command-admin-pending'),
        ('blockjoy-admin', 'host-admin-get'),
        ('blockjoy-admin', 'host-admin-list'),
        ('blockjoy-admin', 'host-admin-regions'),
        ('blockjoy-admin', 'host-admin-update'),
        ('blockjoy-admin', 'image-admin-add'),
        ('blockjoy-admin', 'image-admin-get'),
        ('blockjoy-admin', 'image-admin-list-archives'),
        ('blockjoy-admin', 'image-admin-update-archive'),
        ('blockjoy-admin', 'image-admin-update-image'),
        ('blockjoy-admin', 'invitation-admin-create'),
        ('blockjoy-admin', 'invitation-admin-list'),
        ('blockjoy-admin', 'invitation-admin-revoke'),
        ('blockjoy-admin', 'mqtt-admin-acl'),
        ('blockjoy-admin', 'node-admin-create'),
        ('blockjoy-admin', 'node-admin-delete'),
        ('blockjoy-admin', 'node-admin-get'),
        ('blockjoy-admin', 'node-admin-list'),
        ('blockjoy-admin', 'node-admin-report-error'),
        ('blockjoy-admin', 'node-admin-report-status'),
        ('blockjoy-admin', 'node-admin-restart'),
        ('blockjoy-admin', 'node-admin-start'),
        ('blockjoy-admin', 'node-admin-stop'),
        ('blockjoy-admin', 'node-admin-transfer'),
        ('blockjoy-admin', 'node-admin-update-config'),
        ('blockjoy-admin', 'node-admin-upgrade'),
        ('blockjoy-admin', 'org-address-delete'),
        ('blockjoy-admin', 'org-address-get'),
        ('blockjoy-admin', 'org-address-set'),
        ('blockjoy-admin', 'org-admin-get'),
        ('blockjoy-admin', 'org-admin-list'),
        ('blockjoy-admin', 'org-admin-update'),
        ('blockjoy-admin', 'org-billing-get-billing-details'),
        ('blockjoy-admin', 'org-billing-init-card'),
        ('blockjoy-admin', 'org-billing-list-payment-methods'),
        ('blockjoy-admin', 'protocol-admin-add-protocol'),
        ('blockjoy-admin', 'protocol-admin-add-version'),
        ('blockjoy-admin', 'protocol-admin-get-protocol'),
        ('blockjoy-admin', 'protocol-admin-get-latest'),
        ('blockjoy-admin', 'protocol-admin-list-protocols'),
        ('blockjoy-admin', 'protocol-admin-list-versions'),
        ('blockjoy-admin', 'protocol-admin-update-protocol'),
        ('blockjoy-admin', 'protocol-admin-update-version'),
        ('blockjoy-admin', 'protocol-admin-view-all-stats'),
        ('blockjoy-admin', 'protocol-admin-view-private'),
        ('blockjoy-admin', 'protocol-get-pricing'),
        ('blockjoy-admin', 'user-admin-filter'),
        ('blockjoy-admin', 'user-admin-get'),
        ('blockjoy-admin', 'user-admin-update'),
        ('blockjoy-admin', 'user-settings-admin-delete'),
        ('blockjoy-admin', 'user-settings-admin-get'),
        ('blockjoy-admin', 'user-settings-admin-update'),
        -- api-key-user --
        ('api-key-user', 'api-key-create'),
        ('api-key-user', 'api-key-delete'),
        ('api-key-user', 'api-key-list'),
        ('api-key-user', 'api-key-regenerate'),
        ('api-key-user', 'api-key-update'),
        ('api-key-user', 'user-create'),
        ('api-key-user', 'user-delete'),
        ('api-key-user', 'user-filter'),
        ('api-key-user', 'user-get'),
        ('api-key-user', 'user-settings-delete'),
        ('api-key-user', 'user-settings-get'),
        ('api-key-user', 'user-settings-update'),
        ('api-key-user', 'user-update'),
        -- api-key-org --
        ('api-key-org', 'org-create'),
        ('api-key-org', 'org-get'),
        ('api-key-org', 'org-list'),
        ('api-key-org', 'org-provision-get-token'),
        ('api-key-org', 'org-provision-reset-token'),
        ('api-key-org', 'org-update'),
        -- api-key-host --
        ('api-key-host', 'host-billing-get'),
        ('api-key-host', 'host-delete'),
        ('api-key-host', 'host-get'),
        ('api-key-host', 'host-list'),
        ('api-key-host', 'host-provision-create'),
        ('api-key-host', 'host-provision-get'),
        ('api-key-host', 'host-regions'),
        ('api-key-host', 'host-restart'),
        ('api-key-host', 'host-start'),
        ('api-key-host', 'host-stop'),
        ('api-key-host', 'host-update'),
        ('api-key-host', 'metrics-host'),
        -- api-key-node --
        ('api-key-node', 'command-ack'),
        ('api-key-node', 'command-create'),
        ('api-key-node', 'command-get'),
        ('api-key-node', 'command-list'),
        ('api-key-node', 'command-pending'),
        ('api-key-node', 'command-update'),
        ('api-key-node', 'crypt-get-secret'),
        ('api-key-node', 'crypt-put-secret'),
        ('api-key-node', 'discovery-services'),
        ('api-key-node', 'metrics-node'),
        ('api-key-node', 'node-create'),
        ('api-key-node', 'node-delete'),
        ('api-key-node', 'node-get'),
        ('api-key-node', 'node-list'),
        ('api-key-node', 'node-report-error'),
        ('api-key-node', 'node-report-status'),
        ('api-key-node', 'node-restart'),
        ('api-key-node', 'node-start'),
        ('api-key-node', 'node-stop'),
        ('api-key-node', 'node-update-config'),
        ('api-key-node', 'node-upgrade'),
        ('api-key-node', 'protocol-view-public'),
        -- email-invitation --
        ('email-invitation', 'invitation-accept'),
        ('email-invitation', 'invitation-decline'),
        ('email-invitation', 'user-create'),
        -- email-registration-confirmation --
        ('email-registration-confirmation', 'auth-confirm'),
        -- email-reset-password --
        ('email-reset-password', 'auth-update-password'),
        -- grpc-login --
        ('grpc-login', 'api-key-create'),
        ('grpc-login', 'api-key-delete'),
        ('grpc-login', 'api-key-list'),
        ('grpc-login', 'api-key-regenerate'),
        ('grpc-login', 'api-key-update'),
        ('grpc-login', 'auth-list-permissions'),
        ('grpc-login', 'auth-refresh'),
        ('grpc-login', 'auth-update-ui-password'),
        ('grpc-login', 'bundle-list-versions'),
        ('grpc-login', 'bundle-retrieve'),
        ('grpc-login', 'command-ack'),
        ('grpc-login', 'command-create'),
        ('grpc-login', 'command-get'),
        ('grpc-login', 'command-list'),
        ('grpc-login', 'command-pending'),
        ('grpc-login', 'discovery-services'),
        ('grpc-login', 'image-get'),
        ('grpc-login', 'image-list-archives'),
        ('grpc-login', 'invitation-accept'),
        ('grpc-login', 'invitation-decline'),
        ('grpc-login', 'invitation-list'),
        ('grpc-login', 'metrics-host'),
        ('grpc-login', 'metrics-node'),
        ('grpc-login', 'mqtt-acl'),
        ('grpc-login', 'node-report-error'),
        ('grpc-login', 'org-create'),
        ('grpc-login', 'org-get'),
        ('grpc-login', 'org-list'),
        ('grpc-login', 'org-provision-get-token'),
        ('grpc-login', 'org-provision-reset-token'),
        ('grpc-login', 'protocol-get-protocol'),
        ('grpc-login', 'protocol-get-latest'),
        ('grpc-login', 'protocol-get-pricing'),
        ('grpc-login', 'protocol-list-protocols'),
        ('grpc-login', 'protocol-list-versions'),
        ('grpc-login', 'protocol-view-public'),
        ('grpc-login', 'user-create'),
        ('grpc-login', 'user-delete'),
        ('grpc-login', 'user-filter'),
        ('grpc-login', 'user-get'),
        ('grpc-login', 'user-settings-delete'),
        ('grpc-login', 'user-settings-get'),
        ('grpc-login', 'user-settings-update'),
        ('grpc-login', 'user-update'),
        -- grpc-new-host --
        ('grpc-new-host', 'archive-get-download-chunks'),
        ('grpc-new-host', 'archive-get-download-metadata'),
        ('grpc-new-host', 'archive-get-upload-slots'),
        ('grpc-new-host', 'archive-put-download-manifest'),
        ('grpc-new-host', 'auth-refresh'),
        ('grpc-new-host', 'bundle-list-versions'),
        ('grpc-new-host', 'bundle-retrieve'),
        ('grpc-new-host', 'command-ack'),
        ('grpc-new-host', 'command-create'),
        ('grpc-new-host', 'command-get'),
        ('grpc-new-host', 'command-list'),
        ('grpc-new-host', 'command-pending'),
        ('grpc-new-host', 'command-update'),
        ('grpc-new-host', 'crypt-get-secret'),
        ('grpc-new-host', 'crypt-put-secret'),
        ('grpc-new-host', 'discovery-services'),
        ('grpc-new-host', 'host-get'),
        ('grpc-new-host', 'host-list'),
        ('grpc-new-host', 'host-update'),
        ('grpc-new-host', 'image-get'),
        ('grpc-new-host', 'image-list-archives'),
        ('grpc-new-host', 'metrics-host'),
        ('grpc-new-host', 'metrics-node'),
        ('grpc-new-host', 'mqtt-acl'),
        ('grpc-new-host', 'node-create'),
        ('grpc-new-host', 'node-delete'),
        ('grpc-new-host', 'node-get'),
        ('grpc-new-host', 'node-list'),
        ('grpc-new-host', 'node-report-error'),
        ('grpc-new-host', 'node-report-status'),
        ('grpc-new-host', 'node-restart'),
        ('grpc-new-host', 'node-start'),
        ('grpc-new-host', 'node-stop'),
        ('grpc-new-host', 'node-update-config'),
        ('grpc-new-host', 'node-upgrade'),
        ('grpc-new-host', 'protocol-get-protocol'),
        ('grpc-new-host', 'protocol-get-latest'),
        ('grpc-new-host', 'protocol-list-protocols'),
        ('grpc-new-host', 'protocol-list-versions'),
        ('grpc-new-host', 'protocol-view-public'),
        -- org-owner --
        ('org-owner', 'org-address-delete'),
        ('org-owner', 'org-address-get'),
        ('org-owner', 'org-address-set'),
        ('org-owner', 'org-billing-get-billing-details'),
        ('org-owner', 'org-billing-init-card'),
        ('org-owner', 'org-billing-list-payment-methods'),
        ('org-owner', 'org-delete'),
        -- org-admin --
        ('org-admin', 'crypt-get-secret'),
        ('org-admin', 'crypt-put-secret'),
        ('org-admin', 'host-billing-get'),
        ('org-admin', 'host-delete'),
        ('org-admin', 'invitation-create'),
        ('org-admin', 'invitation-revoke'),
        ('org-admin', 'node-create'),
        ('org-admin', 'node-delete'),
        ('org-admin', 'org-address-delete'),
        ('org-admin', 'org-address-get'),
        ('org-admin', 'org-address-set'),
        ('org-admin', 'org-billing-get-billing-details'),
        ('org-admin', 'org-billing-init-card'),
        ('org-admin', 'org-billing-list-payment-methods'),
        ('org-admin', 'org-remove-member'),
        ('org-admin', 'org-update'),
        ('org-admin', 'protocol-get-pricing'),
        -- org-member --
        ('org-member', 'host-get'),
        ('org-member', 'host-list'),
        ('org-member', 'host-provision-create'),
        ('org-member', 'host-provision-get'),
        ('org-member', 'host-regions'),
        ('org-member', 'host-restart'),
        ('org-member', 'host-start'),
        ('org-member', 'host-stop'),
        ('org-member', 'node-get'),
        ('org-member', 'node-list'),
        ('org-member', 'node-report-error'),
        ('org-member', 'node-restart'),
        ('org-member', 'node-start'),
        ('org-member', 'node-stop'),
        ('org-member', 'node-update-config'),
        ('org-member', 'org-create'),
        ('org-member', 'org-get'),
        ('org-member', 'org-list'),
        ('org-member', 'org-provision-get-token'),
        ('org-member', 'org-provision-reset-token'),
        ('org-member', 'org-remove-self'),
        -- org-personal --
        ('org-personal', 'crypt-get-secret'),
        ('org-personal', 'crypt-put-secret'),
        ('org-personal', 'host-billing-get'),
        ('org-personal', 'host-delete'),
        ('org-personal', 'host-get'),
        ('org-personal', 'host-list'),
        ('org-personal', 'host-provision-create'),
        ('org-personal', 'host-provision-get'),
        ('org-personal', 'host-regions'),
        ('org-personal', 'host-restart'),
        ('org-personal', 'host-start'),
        ('org-personal', 'host-stop'),
        ('org-personal', 'node-create'),
        ('org-personal', 'node-delete'),
        ('org-personal', 'node-get'),
        ('org-personal', 'node-list'),
        ('org-personal', 'node-report-error'),
        ('org-personal', 'node-report-status'),
        ('org-personal', 'node-restart'),
        ('org-personal', 'node-start'),
        ('org-personal', 'node-stop'),
        ('org-personal', 'node-update-config'),
        ('org-personal', 'org-address-delete'),
        ('org-personal', 'org-address-get'),
        ('org-personal', 'org-address-set'),
        ('org-personal', 'org-billing-get-billing-details'),
        ('org-personal', 'org-billing-init-card'),
        ('org-personal', 'org-billing-list-payment-methods'),
        ('org-personal', 'org-create'),
        ('org-personal', 'org-get'),
        ('org-personal', 'org-list'),
        ('org-personal', 'org-provision-get-token'),
        ('org-personal', 'org-provision-reset-token'),
        ('org-personal', 'org-update'),
        ('org-personal', 'protocol-get-pricing'),
        -- view-developer-preview --
        ('view-developer-preview', 'protocol-view-development');
        ";

    diesel::sql_query(query).execute(conn).await.unwrap();
}
