//! Seed new test databases with data for integration testing.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use uuid::Uuid;

use crate::auth::rbac::BlockjoyRole;
use crate::auth::resource::{NodeId, OrgId};
use crate::grpc::common;
use crate::models::blockchain::BlockchainId;
use crate::models::host::{ConnectionStatus, Host, HostType, ManagedBy, MonthlyCostUsd, NewHost};
use crate::models::ip_address::NewIpAddressRange;
use crate::models::node::{Node, NodeProperty, NodeStatus, NodeType, ResourceAffinity};
use crate::models::rbac::RbacUser;
use crate::models::schema::{blockchains, nodes, orgs};
use crate::models::user::NewUser;
use crate::models::{Blockchain, IpAddress, Org, Region, User};

use super::Conn;

pub const ROOT_EMAIL: &str = "root@here.com";
pub const ADMIN_EMAIL: &str = "admin@here.com";
pub const MEMBER_EMAIL: &str = "member@here.com";
pub const UNCONFIRMED_EMAIL: &str = "unconfirmed@here.com";
pub const LOGIN_PASSWORD: &str = "hunter2";

pub const ORG_ID: &str = "08dede71-b97d-47c1-a91d-6ba0997b3cdd";
pub const NODE_ID: &str = "cdbbc736-f399-42ab-86cf-617ce983011d";

pub const HOST_1: &str = "Host-1";
pub const HOST_2: &str = "Host-2";

pub const BLOCKCHAIN_ID: &str = "ab5d8cfc-77b1-4265-9fee-ba71ba9de092";
pub const BLOCKCHAIN_NAME: &str = "Ethereum";
pub const BLOCKCHAIN_NODE_TYPE: &str = "validator";
pub const BLOCKCHAIN_NODE_TYPE_ID: &str = "fb56b151-443b-491a-a2a5-50fc12343a91";
pub const BLOCKCHAIN_VERSION: &str = "3.3.0";
pub const BLOCKCHAIN_VERSION_ID: &str = "a69e7195-8a78-4e3a-a79e-4ac89edf1d68";
pub const BLOCKCHAIN_PROPERTY_KEYSTORE: &str = "5972a35a-333c-421f-ab64-a77f4ae17533";
pub const BLOCKCHAIN_PROPERTY_SELF_HOSTED: &str = "a989ad08-b455-4a57-9fe0-696405947e48";

pub const IP_RANGE_FROM: &str = "127.0.0.1";
pub const IP_RANGE_TO: &str = "127.0.0.10";

pub struct Seed {
    pub user: User,
    pub org: Org,
    pub host: Host,
    pub node: Node,
    pub blockchain: Blockchain,
    pub region: Region,
    pub ip_gateway: String,
    pub ip_addr: String,
}

impl Seed {
    pub async fn new(conn: &mut Conn<'_>) -> Self {
        setup_rbac(conn).await;

        let org = create_orgs(conn).await;
        let user = create_users(&org, conn).await;
        let region = create_region(conn).await;
        let host = create_hosts(&user, org.id, &region, conn).await;
        let blockchain = create_blockchains(conn).await;
        let (ip_gateway, ip_addr) = create_ip_addresses(&host, conn).await;
        let node = create_nodes(org.id, &host, &blockchain, &ip_gateway, &ip_addr, conn).await;

        Seed {
            user,
            org,
            host,
            node,
            blockchain,
            region,
            ip_gateway,
            ip_addr,
        }
    }
}

async fn create_blockchains(conn: &mut Conn<'_>) -> Blockchain {
    let queries = [
        format!("INSERT INTO blockchains (id, name) VALUES ('{BLOCKCHAIN_ID}','{BLOCKCHAIN_NAME}');"),
        format!("INSERT INTO blockchain_node_types (id, blockchain_id, node_type) VALUES ('{BLOCKCHAIN_NODE_TYPE_ID}', '{BLOCKCHAIN_ID}', '{BLOCKCHAIN_NODE_TYPE}');"),
        format!("INSERT INTO blockchain_versions (id, blockchain_id, blockchain_node_type_id, version) VALUES ('{BLOCKCHAIN_VERSION_ID}', '{BLOCKCHAIN_ID}', '{BLOCKCHAIN_NODE_TYPE_ID}', '{BLOCKCHAIN_VERSION}');"),
        format!("INSERT INTO blockchain_properties VALUES ('{BLOCKCHAIN_PROPERTY_KEYSTORE}', '{BLOCKCHAIN_ID}', 'keystore-file', NULL, 'file_upload', FALSE, FALSE, '{BLOCKCHAIN_NODE_TYPE_ID}', '{BLOCKCHAIN_VERSION_ID}', 'Keystore file contents');"),
        format!("INSERT INTO blockchain_properties VALUES ('{BLOCKCHAIN_PROPERTY_SELF_HOSTED}', '{BLOCKCHAIN_ID}', 'self-hosted', NULL, 'switch', FALSE, FALSE, '{BLOCKCHAIN_NODE_TYPE_ID}', '{BLOCKCHAIN_VERSION_ID}', 'Is this noderoni self hosted?');"),
    ];

    for query in queries {
        diesel::sql_query(query).execute(conn).await.unwrap();
    }

    let blockchain_id: BlockchainId = BLOCKCHAIN_ID.parse().unwrap();
    blockchains::table
        .filter(blockchains::id.eq(blockchain_id))
        .get_result(conn)
        .await
        .unwrap()
}

async fn create_orgs(conn: &mut Conn<'_>) -> Org {
    let org_id: OrgId = ORG_ID.parse().unwrap();

    diesel::insert_into(orgs::table)
        .values((
            orgs::id.eq(org_id),
            orgs::name.eq("the blockboys"),
            orgs::is_personal.eq(false),
        ))
        .execute(conn)
        .await
        .unwrap();

    Org::by_id(org_id, conn).await.unwrap()
}

async fn create_users(org: &Org, conn: &mut Conn<'_>) -> User {
    let root = NewUser::new(ROOT_EMAIL, "Super", "Man", LOGIN_PASSWORD)
        .unwrap()
        .create(conn)
        .await
        .unwrap();
    let admin = NewUser::new(ADMIN_EMAIL, "Mr", "Admin", LOGIN_PASSWORD)
        .unwrap()
        .create(conn)
        .await
        .unwrap();
    let member = NewUser::new(MEMBER_EMAIL, "Bog", "Standard", LOGIN_PASSWORD)
        .unwrap()
        .create(conn)
        .await
        .unwrap();
    let _ = NewUser::new(UNCONFIRMED_EMAIL, "Sus", "Guy", LOGIN_PASSWORD)
        .unwrap()
        .create(conn)
        .await
        .unwrap();

    User::confirm(root.id, conn).await.unwrap();
    User::confirm(admin.id, conn).await.unwrap();
    User::confirm(member.id, conn).await.unwrap();

    RbacUser::link_role(root.id, org.id, BlockjoyRole::Admin, conn)
        .await
        .unwrap();

    org.add_admin(admin.id, conn).await.unwrap();
    org.add_member(member.id, conn).await.unwrap();

    User::by_id(admin.id, conn).await.unwrap()
}

async fn create_region(conn: &mut Conn<'_>) -> Region {
    Region::get_or_create("moneyland", conn).await.unwrap()
}

async fn create_hosts(user: &User, org_id: OrgId, region: &Region, conn: &mut Conn<'_>) -> Host {
    let billing = common::BillingAmount {
        amount: Some(common::Amount {
            currency: common::Currency::Usd as i32,
            value: 123,
        }),
        period: common::Period::Monthly as i32,
    };

    let host1 = NewHost {
        name: HOST_1,
        version: "0.1.0",
        cpu_count: 16,
        mem_size_bytes: 1_612_312_312_000,   // 1.6 TB
        disk_size_bytes: 16_121_231_200_000, // 16 TB
        os: "LuukOS",
        os_version: "3",
        ip_addr: "192.168.1.1",
        status: ConnectionStatus::Online,
        ip_range_from: "192.168.0.10".parse().unwrap(),
        ip_range_to: "192.168.0.100".parse().unwrap(),
        ip_gateway: "192.168.0.1".parse().unwrap(),
        org_id,
        created_by: user.id,
        region_id: Some(region.id),
        host_type: HostType::Cloud,
        monthly_cost_in_usd: Some(MonthlyCostUsd::from_proto(&billing).unwrap()),
        vmm_mountpoint: None,
        managed_by: ManagedBy::Automatic,
    };
    let host1 = host1.create(conn).await.unwrap();

    let host2 = NewHost {
        name: HOST_2,
        version: "0.1.0",
        cpu_count: 16,
        mem_size_bytes: 1_612_312_123_123,  // 1.6 TB
        disk_size_bytes: 1_612_312_123_123, // 1.6 TB
        os: "LuukOS",
        os_version: "3",
        ip_addr: "192.168.2.1",
        status: ConnectionStatus::Online,
        ip_range_from: "192.12.0.10".parse().unwrap(),
        ip_range_to: "192.12.0.20".parse().unwrap(),
        ip_gateway: "192.12.0.1".parse().unwrap(),
        org_id,
        created_by: user.id,
        region_id: Some(region.id),
        host_type: HostType::Cloud,
        monthly_cost_in_usd: None,
        vmm_mountpoint: None,
        managed_by: ManagedBy::Automatic,
    };
    host2.create(conn).await.unwrap();

    Host::by_id(host1.id, conn).await.unwrap()
}

async fn create_ip_addresses(host: &Host, conn: &mut Conn<'_>) -> (String, String) {
    NewIpAddressRange::try_new(
        IP_RANGE_FROM.parse().unwrap(),
        IP_RANGE_TO.parse().unwrap(),
        host.id,
    )
    .unwrap()
    .create(&[], conn)
    .await
    .unwrap();

    let ip_gateway = host.ip_gateway.ip().to_string();
    let ip_addr = IpAddress::next_for_host(host.id, conn)
        .await
        .unwrap()
        .ip
        .ip()
        .to_string();

    (ip_gateway, ip_addr)
}

async fn create_nodes(
    org_id: OrgId,
    host: &Host,
    blockchain: &Blockchain,
    ip_gateway: &str,
    ip_addr: &str,
    conn: &mut Conn<'_>,
) -> Node {
    let node_id: NodeId = NODE_ID.parse().unwrap();

    diesel::insert_into(nodes::table)
        .values((
            nodes::id.eq(node_id),
            nodes::name.eq("Test Node"),
            nodes::org_id.eq(org_id),
            nodes::host_id.eq(host.id),
            nodes::blockchain_id.eq(blockchain.id),
            nodes::block_age.eq(0),
            nodes::consensus.eq(true),
            nodes::node_status.eq(NodeStatus::Broadcasting),
            nodes::ip_gateway.eq(ip_gateway),
            nodes::ip_addr.eq(ip_addr),
            nodes::node_type.eq(NodeType::Validator),
            nodes::dns_record_id.eq("The id"),
            nodes::vcpu_count.eq(2),
            nodes::disk_size_bytes.eq(8 * 1024 * 1024 * 1024),
            nodes::mem_size_bytes.eq(1024 * 1024 * 1024),
            nodes::scheduler_resource.eq(ResourceAffinity::LeastResources),
            nodes::version.eq("3.3.0"),
        ))
        .execute(conn)
        .await
        .unwrap();

    let properties = vec![
        NodeProperty {
            id: Uuid::new_v4().into(),
            node_id,
            blockchain_property_id: BLOCKCHAIN_PROPERTY_KEYSTORE.parse().unwrap(),
            value: "Sneaky file content".to_string(),
        },
        NodeProperty {
            id: Uuid::new_v4().into(),
            node_id,
            blockchain_property_id: BLOCKCHAIN_PROPERTY_SELF_HOSTED.parse().unwrap(),
            value: "false".to_string(),
        },
    ];

    NodeProperty::bulk_create(properties, conn).await.unwrap();

    Node::by_id(node_id, conn).await.unwrap()
}

async fn setup_rbac(conn: &mut Conn<'_>) {
    super::create_roles_and_perms(conn).await.unwrap();

    let queries = vec![
        "
        insert into role_permissions (role, permission)
        values
        ('blockjoy-admin', 'auth-admin-list-permissions'),
        ('blockjoy-admin', 'blockchain-admin-get'),
        ('blockjoy-admin', 'blockchain-admin-list'),
        ('blockjoy-admin', 'blockchain-admin-add-node-type'),
        ('blockjoy-admin', 'blockchain-admin-add-version'),
        ('blockjoy-admin', 'host-admin-get'),
        ('blockjoy-admin', 'host-admin-list'),
        ('blockjoy-admin', 'mqtt-admin-acl'),
        ('blockjoy-admin', 'node-admin-create'),
        ('blockjoy-admin', 'node-admin-delete'),
        ('blockjoy-admin', 'node-admin-get'),
        ('blockjoy-admin', 'node-admin-list'),
        ('blockjoy-admin', 'node-admin-report'),
        ('blockjoy-admin', 'node-admin-restart'),
        ('blockjoy-admin', 'node-admin-start'),
        ('blockjoy-admin', 'node-admin-stop'),
        ('blockjoy-admin', 'node-admin-update-config'),
        ('blockjoy-admin', 'node-admin-update-status'),
        ('blockjoy-admin', 'org-admin-get'),
        ('blockjoy-admin', 'org-admin-list'),
        ('blockjoy-admin', 'user-admin-filter'),
        ('blockjoy-admin', 'user-admin-get'),
        ('blockjoy-admin', 'user-admin-update');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('api-key-user', 'user-create'),
        ('api-key-user', 'user-delete'),
        ('api-key-user', 'user-filter'),
        ('api-key-user', 'user-get'),
        ('api-key-user', 'user-update'),
        ('api-key-user', 'user-billing-delete'),
        ('api-key-user', 'user-billing-get'),
        ('api-key-user', 'user-billing-update');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('api-key-org', 'org-create'),
        ('api-key-org', 'org-get'),
        ('api-key-org', 'org-list'),
        ('api-key-org', 'org-update'),
        ('api-key-org', 'org-provision-get-token'),
        ('api-key-org', 'org-provision-reset-token');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('api-key-host', 'host-create'),
        ('api-key-host', 'host-delete'),
        ('api-key-host', 'host-get'),
        ('api-key-host', 'host-list'),
        ('api-key-host', 'host-regions'),
        ('api-key-host', 'host-restart'),
        ('api-key-host', 'host-start'),
        ('api-key-host', 'host-stop'),
        ('api-key-host', 'host-update'),
        ('api-key-host', 'host-billing-get'),
        ('api-key-host', 'host-provision-create'),
        ('api-key-host', 'host-provision-get');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('api-key-node', 'api-key-create'),
        ('api-key-node', 'api-key-delete'),
        ('api-key-node', 'api-key-list'),
        ('api-key-node', 'api-key-regenerate'),
        ('api-key-node', 'api-key-update'),
        ('api-key-node', 'blockchain-get'),
        ('api-key-node', 'blockchain-list'),
        ('api-key-node', 'command-ack'),
        ('api-key-node', 'command-create'),
        ('api-key-node', 'command-get'),
        ('api-key-node', 'command-pending'),
        ('api-key-node', 'command-update'),
        ('api-key-node', 'discovery-services'),
        ('api-key-node', 'key-file-create'),
        ('api-key-node', 'key-file-list'),
        ('api-key-node', 'metrics-host'),
        ('api-key-node', 'metrics-node'),
        ('api-key-node', 'node-create'),
        ('api-key-node', 'node-delete'),
        ('api-key-node', 'node-get'),
        ('api-key-node', 'node-list'),
        ('api-key-node', 'node-report'),
        ('api-key-node', 'node-restart'),
        ('api-key-node', 'node-start'),
        ('api-key-node', 'node-stop'),
        ('api-key-node', 'node-update-config');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('email-invitation', 'invitation-accept'),
        ('email-invitation', 'invitation-decline'),
        ('email-invitation', 'user-create');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('email-registration-confirmation', 'auth-confirm');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('email-reset-password', 'auth-update-password');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('grpc-login', 'api-key-create'),
        ('grpc-login', 'api-key-delete'),
        ('grpc-login', 'api-key-list'),
        ('grpc-login', 'api-key-regenerate'),
        ('grpc-login', 'api-key-update'),
        ('grpc-login', 'auth-list-permissions'),
        ('grpc-login', 'auth-refresh'),
        ('grpc-login', 'auth-update-ui-password'),
        ('grpc-login', 'babel-notify'),
        ('grpc-login', 'blockchain-get'),
        ('grpc-login', 'blockchain-list'),
        ('grpc-login', 'bundle-list-bundle-versions'),
        ('grpc-login', 'bundle-retrieve'),
        ('grpc-login', 'command-ack'),
        ('grpc-login', 'command-create'),
        ('grpc-login', 'command-get'),
        ('grpc-login', 'command-pending'),
        ('grpc-login', 'discovery-services'),
        ('grpc-login', 'invitation-accept'),
        ('grpc-login', 'invitation-decline'),
        ('grpc-login', 'invitation-list'),
        ('grpc-login', 'key-file-create'),
        ('grpc-login', 'key-file-list'),
        ('grpc-login', 'metrics-host'),
        ('grpc-login', 'metrics-node'),
        ('grpc-login', 'mqtt-acl'),
        ('grpc-login', 'node-create'),
        ('grpc-login', 'node-report'),
        ('grpc-login', 'org-create'),
        ('grpc-login', 'org-get'),
        ('grpc-login', 'org-list'),
        ('grpc-login', 'org-provision-get-token'),
        ('grpc-login', 'org-provision-reset-token'),
        ('grpc-login', 'subscription-list'),
        ('grpc-login', 'user-create'),
        ('grpc-login', 'user-delete'),
        ('grpc-login', 'user-filter'),
        ('grpc-login', 'user-get'),
        ('grpc-login', 'user-update'),
        ('grpc-login', 'user-billing-delete'),
        ('grpc-login', 'user-billing-get'),
        ('grpc-login', 'user-billing-update');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('grpc-new-host', 'auth-refresh'),
        ('grpc-new-host', 'babel-notify'),
        ('grpc-new-host', 'blockchain-get'),
        ('grpc-new-host', 'blockchain-get-image'),
        ('grpc-new-host', 'blockchain-get-plugin'),
        ('grpc-new-host', 'blockchain-get-requirements'),
        ('grpc-new-host', 'blockchain-list'),
        ('grpc-new-host', 'blockchain-list-image-versions'),
        ('grpc-new-host', 'blockchain-archive-get-download'),
        ('grpc-new-host', 'blockchain-archive-get-upload'),
        ('grpc-new-host', 'blockchain-archive-put-download'),
        ('grpc-new-host', 'bundle-list-bundle-versions'),
        ('grpc-new-host', 'bundle-retrieve'),
        ('grpc-new-host', 'command-ack'),
        ('grpc-new-host', 'command-create'),
        ('grpc-new-host', 'command-get'),
        ('grpc-new-host', 'command-pending'),
        ('grpc-new-host', 'command-update'),
        ('grpc-new-host', 'discovery-services'),
        ('grpc-new-host', 'host-get'),
        ('grpc-new-host', 'host-list'),
        ('grpc-new-host', 'host-update'),
        ('grpc-new-host', 'kernel-retrieve'),
        ('grpc-new-host', 'key-file-create'),
        ('grpc-new-host', 'key-file-list'),
        ('grpc-new-host', 'metrics-host'),
        ('grpc-new-host', 'metrics-node'),
        ('grpc-new-host', 'mqtt-acl'),
        ('grpc-new-host', 'node-create'),
        ('grpc-new-host', 'node-delete'),
        ('grpc-new-host', 'node-get'),
        ('grpc-new-host', 'node-list'),
        ('grpc-new-host', 'node-report'),
        ('grpc-new-host', 'node-restart'),
        ('grpc-new-host', 'node-start'),
        ('grpc-new-host', 'node-stop'),
        ('grpc-new-host', 'node-update-config'),
        ('grpc-new-host', 'node-update-status');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('org-owner', 'org-delete');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('org-admin', 'host-billing-get'),
        ('org-admin', 'invitation-create'),
        ('org-admin', 'invitation-revoke'),
        ('org-admin', 'org-remove-member'),
        ('org-admin', 'org-update'),
        ('org-admin', 'subscription-create'),
        ('org-admin', 'subscription-delete'),
        ('org-admin', 'subscription-update');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('org-member', 'host-create'),
        ('org-member', 'host-delete'),
        ('org-member', 'host-get'),
        ('org-member', 'host-list'),
        ('org-member', 'host-regions'),
        ('org-member', 'host-restart'),
        ('org-member', 'host-start'),
        ('org-member', 'host-stop'),
        ('org-member', 'host-provision-create'),
        ('org-member', 'host-provision-get'),
        ('org-member', 'node-create'),
        ('org-member', 'node-delete'),
        ('org-member', 'node-get'),
        ('org-member', 'node-list'),
        ('org-member', 'node-report'),
        ('org-member', 'node-restart'),
        ('org-member', 'node-start'),
        ('org-member', 'node-stop'),
        ('org-member', 'node-update-config'),
        ('org-member', 'org-create'),
        ('org-member', 'org-get'),
        ('org-member', 'org-list'),
        ('org-member', 'org-remove-self'),
        ('org-member', 'org-provision-get-token'),
        ('org-member', 'org-provision-reset-token'),
        ('org-member', 'subscription-get');
        ",
        "
        insert into role_permissions (role, permission)
        values
        ('org-personal', 'host-billing-get'),
        ('org-personal', 'host-create'),
        ('org-personal', 'host-delete'),
        ('org-personal', 'host-get'),
        ('org-personal', 'host-list'),
        ('org-personal', 'host-regions'),
        ('org-personal', 'host-restart'),
        ('org-personal', 'host-start'),
        ('org-personal', 'host-stop'),
        ('org-personal', 'host-provision-create'),
        ('org-personal', 'host-provision-get'),
        ('org-personal', 'node-create'),
        ('org-personal', 'node-delete'),
        ('org-personal', 'node-get'),
        ('org-personal', 'node-list'),
        ('org-personal', 'node-report'),
        ('org-personal', 'node-restart'),
        ('org-personal', 'node-start'),
        ('org-personal', 'node-stop'),
        ('org-personal', 'node-update-config'),
        ('org-personal', 'org-create'),
        ('org-personal', 'org-get'),
        ('org-personal', 'org-list'),
        ('org-personal', 'org-provision-get-token'),
        ('org-personal', 'org-provision-reset-token'),
        ('org-personal', 'org-update'),
        ('org-personal', 'subscription-create'),
        ('org-personal', 'subscription-delete'),
        ('org-personal', 'subscription-get'),
        ('org-personal', 'subscription-update');
        ",
    ];

    for query in queries {
        diesel::sql_query(query).execute(conn).await.unwrap();
    }
}
