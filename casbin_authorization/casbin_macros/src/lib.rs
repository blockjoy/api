pub(crate) mod expand;

extern crate proc_macro;

use crate::expand::{FnTemplate, get_altered_fn, parse_args};
use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{parse_macro_input, AttributeArgs, ItemFn};

/// Macro to validate ownership of a given object.
/// If _subject_ is omitted, _user_ will be assumed as default, where _user_ must be
/// an object inside the annotated function and implement ***casbin_authorization::auth::Authorizable***
///
/// # Example
/// ```
/// #[ownership(resource = "node", subject = "user")]
/// async fn some_handler() -> impl axum::response::IntoResponse {
///     (HttpStatusCode::Ok, axum::Json("Resource 'node' belongs to given user"))
/// }
/// ```
#[proc_macro_attribute]
pub fn validate_ownership(args: TokenStream, input: TokenStream) -> TokenStream {
    apply_authorization(args, input, FnTemplate::Ownership)
}

/// Macro to validate current user's privileges to conduct action on object.
/// If _subject_ is omitted, _user_ will be assumed as default, where _user_ must be
/// an object inside the annotated function and implement ***casbin_authorization::auth::Authorizable***
///
/// # Example
/// ```
/// #[validate_privileges(subject = "user", object = "users", action = "create")]
/// async fn some_handler() -> impl axum::response::IntoResponse {
///     (HttpStatusCode::Ok, axum::Json("authorized"))
/// }
/// ```
#[proc_macro_attribute]
pub fn validate_privileges(args: TokenStream, input: TokenStream) -> TokenStream {
    apply_authorization(args, input, FnTemplate::Privileges)
}

/// Shortcut macro for validating both, ownership and privileges.
/// If _subject_ is omitted, _user_ will be assumed as default, where _user_ must be
/// an object inside the annotated function and implement ***casbin_authorization::auth::Authorizable***
///
/// # Example
/// ```
/// #[validate_owner_privileges(subject = "user", object = "users", action = "create", resource = "node")]
/// async fn some_handler() -> impl axum::response::IntoResponse {
///     (HttpStatusCode::Ok, axum::Json("ownership and authorization validated"))
/// }
/// ```
#[proc_macro_attribute]
pub fn validate_owner_privileges(args: TokenStream, input: TokenStream) -> TokenStream {
    apply_authorization(args, input, FnTemplate::Both)
}

fn apply_authorization(args: TokenStream, input: TokenStream, template: FnTemplate) -> TokenStream {
    let parsed_args = parse_args(parse_macro_input!(args as AttributeArgs));
    let func = parse_macro_input!(input as ItemFn);

    get_altered_fn(func, parsed_args, template).into_token_stream().into()
}
