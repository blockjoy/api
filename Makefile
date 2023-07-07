IMAGE_NAME=stakejoy-api
REPO=registry.digitalocean.com/stakejoy
IMAGE_FULL=${REPO}/${IMAGE_NAME}:latest
# APP_ID=2d67787e-f607-4d56-9e8b-5492728086b5

export DATABASE_URL=postgres://blockvisor:password@localhost:25432/blockvisor_db

test:
	@docker-compose up -d
	@cargo test --no-fail-fast
	@docker-compose down

test-with:
	@docker-compose up -d
	@cargo test ${test}
	@docker-compose down

test-flag:
	@docker-compose up -d
	@cargo test --test ${test}
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
