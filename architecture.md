Welcome to the blockvisor api architecture document! In this document we will go
over the resources and concepts comprising the api. The goal of the API is to
instrument several physical or virtual machines that are then able to run
software for us. This software is called a `node` throughout the system, owing
to the history of blockvisor being used to run blockchain nodes. This is however
not a hard requirement; any software may be ran using blockvisor. In order to
fully manage software we need to first define two to download, start, stop,
restart, upgrade and remove it.



### Hosts

A [host](./proto/blockjoy/v1/host.proto:12) represents a machine that is able to
run software for us. These are created through the `bvup` tool, which installs
the blockvisor daemon on a host and registers it with the API.

### Nodes

A [node](./proto/blockjoy/v1/node.proto:14) is a running instance of software.
It's state and metrics will be constantly updated by blockvisor so we can
display the current liveness of the node in the frontend.

### Orgs

An [org](./proto/blockjoy/v1/org.proto:10) represents a group of people using
the blockvisor system. As the org, they can own other resources, such as hosts
and nodes.

### Users

A [user](./proto/blockjoy/v1/user.proto:17) is an account for a person that is
able to interact with the api. For a given organisation that they are a member
of, they will have one or more `roles` assigned, each giving a set of
permissions to interact with the resources of the that org.

### Commands
