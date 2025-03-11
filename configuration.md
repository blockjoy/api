Every config parameter may either be provided from a `config.toml` file or from
an environment parameter. An example config file is provided
[here](./blockvisor-api/config.toml). The full list of environment parameters is
listed below. Any field listed here that has a `Default value` is optional.

### CF_DNS_BASE

Toml path: `cloudflare.dns.base`.
When a node is created a dns entry is created in cloudflare that looks like
`{node_name}.{base}`. So if the base is yourdomain.xyz, and your node's name is
mynewnode, then the dns entry is an A record pointing `mynewnode.yourdomain.xyz`
to the IP of the node.

### CF_TTL

Toml path: `cloudflare.dns.ttl`
For the DNS record created in the circumstances described above, this is the
ttl.

### CF_ZONE

Toml path: `cloudflare.api.zone_id`
The cloudflare id of the zone where the DNS record is created.

### CF_TOKEN

Toml path: `cloudflare.api.token`
The cloudflare API access token. This is passed to cloudflare in the
Authorization header with `Bearer ` prefixed to it on each request.

### DATABASE_URL

Toml path: `database.url`
A URL to the postgres database that the backend can use.

### DB_MAX_CONN

Toml path: `database.max_conns`
Default value: 10
The max number of database connections that the connection pool is allowed to
grow to.

### DB_MIN_CONN

Toml path: `database.min_conns`
Default value: 2
The minimum number of database connections that the connection pool is allowed to
shrink to.

### DB_MAX_LIFETIME

Toml path: `database.max_lifetime`
Default value: 1d
The maximum duration for which a database connection should be kept before it is
closed and a new one should be opened.

### DB_IDLE_TIMEOUT

Toml path: `database.idle_timeout`
Default value: 2m
If a connection is idle for this duration, and the current number of connections
is greater than `min_conns`, it is closed.

### EMAIL_TEMPLATE_DIR

Toml path: `email.template_dir`
The directory containing the renderable templates of the emails that the backend
sends.

### SENDGRID_API_KEY

Toml path: `email.sendgrid_api_key`
The API key used for interaction with sendgrid.

### UI_BASE_URL

Toml path: `email.ui_base_url`
The url at which the frontend of the API is served. This is used for rendering
clickable links in the emails, such as `https://{ui_base_url}/register`.

### REQUEST_CONCURRENCY_LIMIT

Toml path: `grpc.request_concurrency_limit`
Default value: 32
The max number of concurrent requests we allow per incoming connection.

### LOG_ENVIRONMENT

Toml path: `log.environment`
The current environment that we are in, either `dev`, `staging` or `production`.
The logging environment in included in every log line.

### LOG_FILTER

Toml path: `log.filter`
Default value: `info`
The logging filter string. For syntax:
see https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html

### OPENTELEMETRY_ENDPOINT

Toml path: `log.opentelemetry.endpoint`
The opentelemetry endpoint to post the logs to.

### OPENTELEMETRY_EXPORT_INTERVAL

Toml path: `log.opentelemetry.export_interval`
Default value: `5s`
Denotes how often the service should send its logs to the opentelemetry.

### MQTT_SERVER_ADDRESS

Toml path: `mqtt.server_address`
On startup the service will connect to an MQTT server, where it is expected to
be allowed to post messages on any topic. The address of the server is stored in
this variable.

### MQTT_SERVER_PORT

Toml path: `mqtt.server_port`
The port of the aforementioned MQTT server.

### MQTT_USERNAME

Toml path: `mqtt.username`
The username that the service should use to authenticate itself at the MQTT
server.

### MQTT_PASSWORD

Toml path: `mqtt.password`
The password that the service should use to authenticate itself at the MQTT
server.

### SECRETS_ROOT

Toml path: <this parameter cannot be configured through the toml file>
Denotes the path to the toml file that contains the config parameters. Since
this is how we find the toml file, this parameter has to be set through a
environment variable.

### CLOUDFLARE_CERT_KEY

Toml path: `secret.cloudflare_cert_key`
When blockvisor asks for a secret called `cloudflare-cert-key`, this is the
value that we return.

### GRAFANA_LOKI_KEY

Toml path: `secret.grafana_loki_key`
When blockvisor asks for a secret called `grafana-loki-key`, this is the value
that we return.

### GRAFANA_PROMETHEUS_KEY

Toml path: `secret.grafana_prometheus_key`
When blockvisor asks for a secret called `grafana-prometheus-key`, this is the
value that we return.

### BIND_IP

Toml path: `server.ip`
Currently unused.

### PORT

Toml path: `server.port`
Default: 8080
Currently unused.

### AWS_ACCESS_KEY_ID

Toml path: `store.aws_access_key_id`
Used to authenticate requests to the S3-like service.

### WS_SECRET_ACCESS_KEY

Toml path: `store.aws_secret_access_key`
Used to authenticate requests to the S3-like service.

### DIR_CHAINS_PREFIX

Toml path: `store.prefix`
Currently unused.

### PRESIGNED_URL_EXPIRATION

Toml path: `store.expiration`
The amount of time that download and upload urls should be valid for. This is
denoted in a time formatted like `1s`, or `2d`.

### AWS_REGION

Toml path: `store.aws_region`
The AWS region that the service is in.

### STORAGE_URL

Toml path: `store.url`
The url to the AWS endpoind that we should talk to. This can be used to swap AWS
for an S3-compatible service.

### ARCHIVE_BUCKET

Toml path: `store.bucket.archive`
The bucket that we use to store our blockchain archives.

### BUNDLE_BUCKET

Toml path: `store.bucket.bundle`
The bucket where we store our blockvisor bundles.

### STRIPE_SECRET

Toml path: `stripe.secret`
The stripe private key.

### STRIPE_URL

Toml path: `stripe.url`
The url to the stripe service api.

### JWT_SECRET

Toml path: `token.secret.jwt`
The secret used to sign our login JWT's.

### REFRESH_SECRET

Toml path: `token.secret.refresh`
The secret used to sign our refresh JWT's.

### JWT_SECRET_FALLBACK

Toml path: `token.secret.jwt_fallback`
When validating a token with `JWT_SECRET` fails, then we use this value as a
secret and still accept the token if that makes the validation passes. This
allows us to gracefully migrate to a new `JWT_SECRET` without logging everybody
out. When everyone refreshes, all tokens will be signed with the new JWT_SECRET,
and this value can be unset.

### REFRESH_SECRET_FALLBACK

Toml path: `token.secret.refresh_fallback`
This value works the same as described above, but for the refresh secret.

### TOKEN_EXPIRATION_MINS

Toml path: `token.expire.token`
Default value: 10m



### REFRESH_HOST_EXPIRE

Toml path: `token.expire.refresh_host`
Default value: 30d
The expiration time of the refresh tokens that we issue to hosts. If this value
is not set, before it falls back to its default value, it will check whether an
environment parameter called `REFRESH_EXPIRATION_HOST_MINS` is set, and
interpret that as number of minutes.

### REFRESH_USER_EXPIRE

Toml path: `token.expire.refresh_user`
Default value: 20h
The expiration time of the refresh tokens that we issue to users. If this value
is not set, before it falls back to its default value, it will check whether an
environment parameter called `REFRESH_EXPIRATION_USER_MINS` is set, and
interpret that as number of minutes.

### PASSWORD_RESET_EXPIRE

Toml path: `token.expire.password_reset`
Default value: 5m
The expiration time of the refresh tokens that we put in password reset emails.
If this value is not set, before it falls back to its default value, it will
check whether an environment parameter called `PWD_RESET_EXPIRATION_MINS` is
set, and interpret that as number of minutes.

### REGISTRATION_CONFIRMATION_EXPIRE

Toml path: `token.expire.registration_confirmation`
Default value: 30m
The expiration time of the refresh tokens that we put in registration / email
confirmation emails. If this value is not set, before it falls back to its
default value, it will check whether an environment parameter called
`REGISTRATION_CONFIRMATION_MINS` is set, and interpret that as number of
minutes.

### INVITATION_EXPIRE

Toml path: `token.expire.invitation`
Default value: 168m
The expiration time of the refresh tokens that we put in organisation invitation
emails. If this value is not set, before it falls back to its default value, it
will check whether an environment parameter called `INVITATION_MINS` is set, and
interpret that as number of minutes.
