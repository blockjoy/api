/**
 * Helper lib providing types for node-types and their actually values stored in nodes
 *
 */

/**
 * All possible node-type IDs (taken from src/models/node_type.rs)
 */
export enum NodeTypeKey {
    Miner = 1,
    Etl = 2,
    Validator = 3,
    Api = 4,
    Oracle = 5,
    Relay = 6,
    Execution = 7,
    Beacon = 8,
    MevBoost = 9,
    Node = 10,
    FullNode = 11,
    LightNode = 12,
}

/**
 * All possible UI types provided for nodes
 */
export enum UiType {
    // Upload e.g. validator keys component
    KeyUpload = 'key-upload',
    // Upload arbitrary files component
    FileUpload = 'file-upload',
    // Enter arbitrary text
    Text = 'text',
    // Enter some password
    Password = "password",
    // Enter arbitrary numbers
    Number = 'number',
    // Enter a voting key pwd
    VotingKeyPwd = 'voting_key_pwd',
    // Enter a wallet address
    WalletAddress = 'wallet_address',
    // Render a switch
    Switch = 'switch',
    // Render a URL
    Url = 'url',
    // Save an IPv4 address
    IpAddress = 'ip_address',
}

/**
 * A single node property template
 */
export type Property = {
    // Property name as used in the resulting node
    name: string,
    // Type of UI element to input value
    ui_type: UiType,
    // Optional default value
    default: string,
    // Indicates whether the field should be read-only
    disabled: boolean,
    // Indicates whether the field is required
    required: boolean,
};

/**
 * A node-type template
 */
export type NodeType = {
    // The ID of the node-type matching the enum in the DB
    id: NodeTypeKey,
    // The node version
    version: string,
    // An optional array of node-type properties
    properties: Array<Property> | null,
};

/**
 * Collection of all supported node-types among a blockchain
 */
export type SupportedNodeTypes = Array<NodeType>;

/**
 * Node property value storing an actual field of one node property
 */
export type NodePropertyValue = {
    // The technical ID of the value field
    id: string,
    // The display name of the value field
    label: string,
    // Optional descriptive text (e.g. shown in a tooltip)
    description: string | null,
    // The actual value
    value: any | null,
    // The UI type for rendering
    ui_type: UiType,
    // Determines if value is read-only
    disabled: boolean,
};

/**
 * Collection of NodeProperty values to be created within the UI
 */
export type NodeProperties = {
    id: NodeTypeKey,
    values: Array<NodePropertyValue> | null,
};

/// Samples
const disabled_self_hosted: Property = {
    name: 'self-hosted',
    ui_type: UiType.Switch,
    default: "false",
    disabled: true,
    required: true,
};

////////////////////////////////////////////////////
///             HNT sample                       ///
////////////////////////////////////////////////////

const hnt_miner: NodeType = {
    id: NodeTypeKey.Miner,
    version: '1.17.2-build.5',
    properties: [
        {
            name: 'keystore-file',
            ui_type: UiType.KeyUpload,
            default: "",
            disabled: false,
            required: true,
        },
        disabled_self_hosted
    ],
};

const hnt_validator: NodeType = {
    id: NodeTypeKey.Validator,
    version: '1.17.2-build.5',
    properties: [
        {
            name: 'keystore-file',
            ui_type: UiType.KeyUpload,
            default: "",
            disabled: false,
            required: true,
        },
        disabled_self_hosted
    ],
};

////////////////////////////////////////////////////
///             ALGO sample                       ///
////////////////////////////////////////////////////

const algo_node: NodeType = {
    id: NodeTypeKey.Node,
    version: '3.14.2',
    properties: [
        disabled_self_hosted
    ],
};

////////////////////////////////////////////////////
///             ADA sample                       ///
////////////////////////////////////////////////////

const ada_full: NodeType = {
    id: NodeTypeKey.Node,
    version: '1.35.5-build.1',
    properties: [
        disabled_self_hosted
    ],
};

const ada_validator: NodeType = {
    id: NodeTypeKey.Validator,
    version: '0.0.3',
    properties: [
        disabled_self_hosted
    ],
};

////////////////////////////////////////////////////
///             ETH sample                       ///
////////////////////////////////////////////////////

const eth_validator: NodeType = {
    id: NodeTypeKey.Validator,
    version: '3.30.2-build.5',
    properties: [
        {
            name: 'keystore-file-1',
            ui_type: UiType.KeyUpload,
            default: "",
            disabled: false,
            required: true,
        },
        {
            name: 'keystore-file-2',
            ui_type: UiType.KeyUpload,
            default: "",
            disabled: false,
            required: false,
        },
        {
            name: 'keystore-file-3',
            ui_type: UiType.KeyUpload,
            default: "",
            disabled: false,
            required: false,
        },
        {
            name: 'voting-pwd',
            ui_type: UiType.VotingKeyPwd,
            default: "",
            disabled: false,
            required: false,
        },
        {
            name: 'fee-recipient',
            ui_type: UiType.WalletAddress,
            default: "",
            disabled: false,
            required: true,
        },
        {
            name: 'mev-boost',
            ui_type: UiType.Switch,
            default: "",
            disabled: false,
            required: false,
        },
        disabled_self_hosted,
    ],
};

function write_json(chain: string, node_types: SupportedNodeTypes) {
    const path = `./${chain}_supported_node_types.json`;
    let json = JSON.stringify(node_types);

    Deno.writeTextFile(path, json).then(() => {
        console.log(`Created JSON file in ${path}`);
    }).catch((e) => {
        console.error(`Failed writing JSON file in ${path}: ${e}`);
    });
}

// Write HNT node types
// write_json("HNT", [hnt_validator, hnt_miner]);

// Write ETH node types
// write_json("ETH", [eth_validator]);

// Write ADA node types
// write_json("ADA", [ada_full, ada_validator]);

// Write ALGO node types
write_json("ALGO", [algo_node]);
