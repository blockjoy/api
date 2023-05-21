# Creating supported node types JSON

## Prerequisites

Deno must be installed

## Workflow

1. Use `node_types.schema.ts` to create the intended data structures
2. When finished, `cd` into this directory and run
   ```bash
    deno run --allow-write node_types.schema.ts
   ```
3. This will create `supported_node_types.json` which can then be used inside the Blockchains table `blockchains.supported_node_types`
