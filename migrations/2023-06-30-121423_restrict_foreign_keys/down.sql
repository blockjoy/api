ALTER TABLE nodes
    DROP CONSTRAINT nodes_host_id_fkey,
    ADD CONSTRAINT nodes_host_id_fkey FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE,

    DROP CONSTRAINT nodes_org_id_fkey,
    ADD CONSTRAINT nodes_org_id_fkey FOREIGN KEY (org_id) REFERENCES orgs(id) ON DELETE CASCADE,

    DROP CONSTRAINT nodes_blockchain_id_fkey,
    ADD CONSTRAINT nodes_blockchain_id_fkey FOREIGN KEY (blockchain_id) REFERENCES blockchains(id) ON DELETE CASCADE;
