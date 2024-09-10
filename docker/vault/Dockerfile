FROM debian:bookworm

RUN apt-get update && \
    apt-get install -y gpg libcap2-bin wget && \
    wget -O- https://apt.releases.hashicorp.com/gpg | gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg && \
    echo "deb [arch=amd64 signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com bookworm main" | tee /etc/apt/sources.list.d/hashicorp.list && \
    apt-get update && \
    apt-get install -y vault && \
    setcap cap_ipc_lock=+ep /usr/bin/vault

COPY init.sh ./
RUN chmod +x ./init.sh

CMD ./init.sh
