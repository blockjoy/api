# Casbin authorization macros

## Ownership


Macro to validate ownership of a given object.
If _subject_ is omitted, _user_ will be assumed as default, where _user_ must be
an object inside the annotated function and implement ***casbin_authorization::auth::Authorizable***

### Example


```rust
#[ownership(resource = "node", subject = "user")]
async fn some_handler() -> impl IntoResponse {
    (HttpStatusCode::Ok, Json("Resource 'node' belongs to given user"))
}
```

## Privileges


Macro to validate current user's privileges to conduct action on object.
If _subject_ is omitted, _user_ will be assumed as default, where _user_ must be
an object inside the annotated function and implement ***casbin_authorization::auth::Authorizable***

### Example


```rust
#[validate_privileges(subject = "user", object = "users", action = "create")]
async fn some_handler() -> impl IntoResponse {
    (HttpStatusCode::Ok, Json("authorized"))
}
```

## Ownership and privileges


Shortcut macro for validating both, ownership and privileges.
If _subject_ is omitted, _user_ will be assumed as default, where _user_ must be
an object inside the annotated function and implement ***casbin_authorization::auth::Authorizable***

### Example


```rust
#[validate_owner_privileges(subject = "user", object = "users", action = "create", resource = "node")]
async fn some_handler() -> impl IntoResponse {
    (HttpStatusCode::Ok, Json("ownership and authorization validated"))
}
```

## Improvements


### Loading configuration once at startup


Currently, each time #apply_authorization is called, the config is read from disk. That should be improved by providing
an Authorization instance to the server once, so that the config are only read once.