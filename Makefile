IMAGE_NAME=stakejoy-api
REPO=registry.digitalocean.com/stakejoy
IMAGE_FULL=${REPO}/${IMAGE_NAME}:latest
# APP_ID=2d67787e-f607-4d56-9e8b-5492728086b5

export DATABASE_URL=postgres://blockvisor:password@localhost:25432/blockvisor_db
export DATABASE_URL_NAKED=postgres://blockvisor:password@localhost:25432
export JWT_SECRET=123456
export API_SERVICE_SECRET=abc123

export MQTT_SERVER_ADDRESS=35.237.162.218
export MQTT_SERVER_PORT=1883
export MQTT_USERNAME=blockvisor-api
export MQTT_PASSWORD=PH*rE:\ZQlecB9/I?[#R$q3M;5yCb]Y+
export KEY_SERVICE_URL=henk
# Cloudflare
export CF_BASE_URL=https://api.cloudflare.com/client/v4
export CF_ZONE=89560cdd783e35f7a9d718755ea9c656
export CF_DNS_BASE=n0des.xyz
export CF_TTL=300
# secret
export CF_TOKEN=9QjEiXC4B26tgshHZjuZ57kJcjaChSSsDfzUvfYQ

export TOKEN_EXPIRATION_MINS=10
export REFRESH_TOKEN_EXPIRATION_MINS=10

test:
	@docker-compose up -d
	@cargo test --no-fail-fast
	@docker-compose down

test-with:
	@docker-compose up -d
	@cargo test ${test}
	@docker-compose down

start-db:
	@docker-compose up -d
	@sleep 2
	@diesel migration run
	@echo ""
	@echo " ---------------------------------------------------"
	@echo "| WARN: PLEASE RUN 'make stop-db' AFTER YOU'RE DONE |"
	@echo " ---------------------------------------------------"

stop-db:
	@docker-compose down

# docker-build:
#	@docker build . -t ${IMAGE_NAME}

# docker-push:
#	@docker tag ${IMAGE_NAME} ${IMAGE_FULL}
#	@docker push ${IMAGE_FULL}

# deploy: docker-build docker-push
#	@doctl apps create-deployment ${APP_ID} --wait
