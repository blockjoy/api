use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{AttributeArgs, ItemFn, NestedMeta};

pub(crate) enum FnTemplate {
    Ownership,
    Privileges,
    Both,
}

pub(crate) struct AuthMacroArgs {
    object: Option<syn::Expr>,
    subject: Option<syn::Expr>,
    action: Option<syn::Expr>,
    resource: Option<syn::Expr>,
}

impl AuthMacroArgs {
    pub fn new(args: AttributeArgs) -> syn::Result<Self> {
        let mut subject = None;
        let mut object = None;
        let mut action = None;
        let mut resource = None;

        for arg in args {
            match arg {
                // arg is a literal
                NestedMeta::Lit(syn::Lit::Str(lit)) => {
                    return Err(syn::Error::new_spanned(lit, "Unsupported value"));
                }
                NestedMeta::Meta(syn::Meta::NameValue(syn::MetaNameValue {
                    path,
                    lit: syn::Lit::Str(lit_str),
                    ..
                })) => {
                    if path.is_ident("subject") {
                        let expr = lit_str.parse().unwrap();
                        subject = Some(expr);
                    } else if path.is_ident("resource") {
                        let expr = lit_str.parse().unwrap();
                        resource = Some(expr);
                    } else if path.is_ident("object") {
                        let expr = lit_str.parse().unwrap();
                        object = Some(expr);
                    } else if path.is_ident("action") {
                        let expr = lit_str.parse().unwrap();
                        action = Some(expr);
                    } else {
                        return Err(syn::Error::new_spanned(
                            path,
                            "Unknown identifier. Available: 'subject', 'object', 'action'",
                        ));
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(arg, "Unknown attribute."));
                }
            }
        }

        Ok(Self {
            subject,
            object,
            action,
            resource,
        })
    }
}

/// Helper parsing attribute args
pub(crate) fn parse_args(args: AttributeArgs) -> AuthMacroArgs {
    match AuthMacroArgs::new(args) {
        Ok(args) => args,
        Err(_) => panic!("Can't parse args"),
    }
}

/// Generate new body for annotated function
///
/// TODO: Refactor fn so the altered fn templates don't all reside in here. Problem being using
///     e.g. a struct for template parameters requires that struct to implement ToTokenStream
pub(crate) fn get_altered_fn(current_fn: ItemFn, args: AuthMacroArgs, template: FnTemplate) -> TokenStream {
    let fn_vis = current_fn.vis;
    let fn_block = current_fn.block;

    // Function signature
    let fn_sig = current_fn.sig;
    // Additional outer attributes to the function (passed via another proc macro)
    let fn_attrs = current_fn.attrs;
    // Function name
    let fn_name = &fn_sig.ident;
    // Generic params for function
    let fn_generics = &fn_sig.generics;
    // Additional function parameters
    let fn_args = &fn_sig.inputs;
    // Is function async?
    let fn_async = &fn_sig.asyncness.unwrap();
    let fn_output = &fn_sig.output;

    // Get auth args from attribute args
    let subject = args
        .subject
        .as_ref()
        .map(|t| t.to_token_stream())
        // Use "user" as default object if subject is omitted
        .unwrap_or(quote! { user });
    let resource = args
        .resource
        .as_ref()
        .map(|t| t.to_token_stream())
        .unwrap_or(quote! { String });
    let object = args
        .object
        .as_ref()
        .map(|t| t.to_token_stream().to_string())
        .unwrap_or("".to_string());
    let action = args
        .action
        .as_ref()
        .map(|t| t.to_token_stream().to_string())
        .unwrap_or("".to_string());

    match template {
        FnTemplate::Both => {
            quote! {
                #(#fn_attrs)*
                #fn_vis #fn_async fn #fn_name #fn_generics(
                    #fn_args
                ) #fn_output {
                    match #resource.is_owned_by(#subject) {
                        casbin_authorization::auth::OwnershipState::Owned => {
                            let auth = casbin_authorization::auth::Authorization::new().await.unwrap();

                            match auth.try_authorized(#subject.get_role(), String::from(#object), String::from(#action)) {
                                Ok(state) => {
                                    match state {
                                        auth::AuthorizationState::Authorized => {
                                            #fn_block
                                        },
                                        auth::AuthorizationState::Denied => {
                                            Ok((StatusCode::FORBIDDEN, Json(vec![])))
                                        }
                                    }
                                },
                                Err(err) => {
                                    Ok((StatusCode::FORBIDDEN, Json(vec![])))
                                }
                            }
                        },
                        casbin_authorization::auth::OwnershipState::NotOwned => {
                            Ok((StatusCode::FORBIDDEN, Json(vec![])))
                        },
                    }
                }
            }
        },
        FnTemplate::Ownership => {
            quote! {
                #(#fn_attrs)*
                #fn_vis #fn_async fn #fn_name #fn_generics(
                    #fn_args
                ) #fn_output {
                    match #resource.is_owned_by(#subject) {
                        casbin_authorization::auth::OwnershipState::Owned => {
                            #fn_block
                        }
                        casbin_authorization::auth::OwnershipState::NotOwned => {
                            Ok((StatusCode::FORBIDDEN, Json(vec![])))
                        }
                    }
                }
            }
        },
        FnTemplate::Privileges => {
            quote! {
                #(#fn_attrs)*
                #fn_vis #fn_async fn #fn_name #fn_generics(
                    #fn_args
                ) #fn_output {
                    let auth = casbin_authorization::auth::Authorization::new().await.unwrap();

                    match auth.try_authorized(#subject.get_role(), String::from(#object), String::from(#action)) {
                        Ok(state) => {
                            match state {
                                auth::AuthorizationState::Authorized => {
                                    #fn_block
                                },
                                auth::AuthorizationState::Denied => {
                                    Ok((StatusCode::FORBIDDEN, Json(vec![])))
                                }
                            }
                        },
                        Err(err) => {
                            Ok((StatusCode::FORBIDDEN, Json(vec![])))
                        }
                    }
                }
            }
        },
    }
}
