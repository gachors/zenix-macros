use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, ItemFn, Lit, Meta, MetaNameValue, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

// ---------------------------------------------------------------------------
// Attribute arguments
// ---------------------------------------------------------------------------

/// Parsed form of `#[openapi(path = "/foo", method = "get", ...)]`.
struct OpenApiArgs {
    path: String,
    method: String,
    tag: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    deprecated: bool,
    response_desc: Option<String>,
}

impl Parse for OpenApiArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut path: Option<String> = None;
        let mut method: Option<String> = None;
        let mut tag: Option<String> = None;
        let mut summary: Option<String> = None;
        let mut description: Option<String> = None;
        let mut deprecated = false;
        let mut response_desc: Option<String> = None;

        // Parse comma-separated `key = value` or bare `key` flags
        let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;

        for meta in metas {
            match meta {
                Meta::NameValue(MetaNameValue {
                    path: key, value, ..
                }) => {
                    let name = key
                        .get_ident()
                        .ok_or_else(|| syn::Error::new(key.span(), "expected an identifier"))?
                        .to_string();

                    let val_str = match value {
                        syn::Expr::Lit(syn::ExprLit {
                            lit: Lit::Str(s), ..
                        }) => s.value(),
                        _ => {
                            return Err(syn::Error::new(value.span(), "expected a string literal"));
                        }
                    };

                    match name.as_str() {
                        "path" => path = Some(val_str),
                        "method" => method = Some(val_str),
                        "tag" => tag = Some(val_str),
                        "summary" => summary = Some(val_str),
                        "description" => description = Some(val_str),
                        "response_desc" => response_desc = Some(val_str),
                        _ => {
                            return Err(syn::Error::new(
                                key.span(),
                                format!("unknown key `{name}`"),
                            ));
                        }
                    }
                }
                Meta::Path(p) => {
                    let name = p
                        .get_ident()
                        .ok_or_else(|| syn::Error::new(p.span(), "expected an identifier"))?
                        .to_string();
                    if name == "deprecated" {
                        deprecated = true;
                    } else {
                        return Err(syn::Error::new(p.span(), format!("unknown flag `{name}`")));
                    }
                }
                _ => {
                    return Err(syn::Error::new(
                        meta.span(),
                        "expected `key = \"value\"` or bare flag",
                    ));
                }
            }
        }

        let path = path.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "missing required `path` argument for #[openapi]",
            )
        })?;

        Ok(Self {
            path,
            method: method.unwrap_or_else(|| "get".into()),
            tag,
            summary,
            description,
            deprecated,
            response_desc,
        })
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Attribute macro that annotates an async handler function with OpenAPI
/// metadata and generates a hidden registration helper.
///
/// # Example
///
/// ```ignore
/// #[zenix::openapi(path = "/hello", method = "get", tag = "Greetings")]
/// async fn hello() -> impl Serialize {
///     serde_json::json!({ "message": "Hello!" })
/// }
/// ```
#[proc_macro_attribute]
pub fn openapi(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args: OpenApiArgs = match syn::parse(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let item_fn: ItemFn = match syn::parse(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };

    expand(args, item_fn)
}

fn expand(args: OpenApiArgs, item_fn: ItemFn) -> TokenStream {
    let fn_name = &item_fn.sig.ident;
    let fn_vis = &item_fn.vis;

    // Hide existing #[openapi(...)] attribute from the output function so we
    // don't recurse — keep all other attributes.
    let other_attrs: Vec<&Attribute> = item_fn
        .attrs
        .iter()
        .filter(|a| !a.path().is_ident("openapi"))
        .collect();

    let register_fn_name = format_ident!("__zenix_register_{}", fn_name);
    let route_const_name = format_ident!("{}_route", fn_name);

    let path = &args.path;
    let method_str = &args.method;

    // Build the describe chain
    let method_ident = format_ident!("{}", method_str);
    let tag_call = args.tag.as_ref().map(|t| {
        quote! { .tag(#t) }
    });
    let summary_call = args.summary.as_ref().map(|s| {
        quote! { .summary(#s) }
    });
    let description_call = args.description.as_ref().map(|d| {
        quote! { .description(#d) }
    });
    let deprecated_call = if args.deprecated {
        Some(quote! { .deprecated() })
    } else {
        None
    };
    let response_desc = args.response_desc.as_deref().unwrap_or("Success response");
    let response_call = Some(quote! {
        .json_response::<serde_json::Value>(#response_desc)
    });

    // We lowercase the method name for the server call: get, post, put, etc.
    let server_method = format_ident!("{}", method_str.to_lowercase());

    let expanded = quote! {
        #(#other_attrs)*
        #fn_vis #item_fn

        #[doc(hidden)]
        #[allow(non_snake_case)]
        #fn_vis fn #register_fn_name(server: &mut ::zenix::Server) {
            server.#server_method(#path, #fn_name);
            server.describe(
                ::zenix::openapi::path::#method_ident(#path)
                    #tag_call
                    #summary_call
                    #description_call
                    #deprecated_call
                    #response_call
            );
        }

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        #fn_vis const #route_const_name: ::zenix::RouteHandle =
            ::zenix::RouteHandle::new(#register_fn_name);
    };

    expanded.into()
}
