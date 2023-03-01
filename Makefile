IMAGE_NAME=stakejoy-api
REPO=registry.digitalocean.com/stakejoy
IMAGE_FULL=${REPO}/${IMAGE_NAME}:latest
# APP_ID=2d67787e-f607-4d56-9e8b-5492728086b5

export DATABASE_URL=postgres://blockvisor:password@localhost:25432/blockvisor_db
export DATABASE_URL_NAKED=postgres://blockvisor:password@localhost:25432
export JWT_SECRET=123456
export API_SERVICE_SECRET=abc123

test: 
	@docker-compose up -d
	@cargo test
	@docker-compose down

# docker-build:
#	@docker build . -t ${IMAGE_NAME}

# docker-push:
#	@docker tag ${IMAGE_NAME} ${IMAGE_FULL}
#	@docker push ${IMAGE_FULL}

# deploy: docker-build docker-push
#	@doctl apps create-deployment ${APP_ID} --wait
