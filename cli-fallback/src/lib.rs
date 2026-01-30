extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, GenericArgument, Lit, Meta, PathArguments, Type,
    parse_macro_input,
};

// #[proc_macro_attribute]
// pub fn comment(_attr: TokenStream, item: TokenStream) -> TokenStream {
//     let mut input = parse_macro_input!(item as ItemStruct);

//     for Field { attrs, .. } in &mut input.fields {
//         attrs.push(syn::parse_quote!(#[doc = "I am generated"]));
//     }

//     quote!(#input).into()
// }

#[proc_macro_derive(Fallback, attributes(fallback))]
pub fn fallback(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let methods = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields
                .named
                .into_iter()
                .filter_map(|f| {
                    let name = f.ident.unwrap();
                    let fty = f.ty;

                    let fallback_expr = f.attrs.iter().find(|a| a.path().is_ident("fallback"))?;
                    let expr = parse_fallback_expr(fallback_expr)
                        .expect("Fallback value must be provided, for example: #[fallback(true)]");

                    assert_doc_comment(&f.attrs, &expr);

                    let body = quote! {
                        self.#name.clone().unwrap_or(#expr)
                    };

                    let unwrapped_type = unwrap_option_type(fty);
                    Some(quote! {
                        pub fn #name(&self) -> #unwrapped_type {
                            #body
                        }
                    })
                })
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };

    quote! {
        impl #name {
            #(#methods)*
        }
    }
    .into()
}

fn assert_doc_comment(attrs: &[Attribute], expr: &Expr) {
    let last_doc_line = attrs.iter().rev().find(|a| a.path().is_ident("doc"));
    let attr = last_doc_line.expect("Doc comment is required");

    // match expr {
    //     Expr::Lit(expr_lit) => {
    //         expr_lit.lit.
    //     },
    //     _ => todo!("Not lit"),
    // };

    let text = match &attr.meta {
        Meta::NameValue(nv) => match &nv.value {
            Expr::Lit(l) => match &l.lit {
                Lit::Str(s) => s.value(),
                _ => panic!("Invalid doc comment"),
            },
            _ => panic!("Invalid doc comment"),
        },
        _ => panic!("Invalid doc comment"),
    };

    let expected_suffix = format!("fallback: {}", expr.to_token_stream());
    if !text.trim().ends_with(&expected_suffix) {
        panic!(
            "Expected doc comment to end with `{}` but found `{}`",
            expected_suffix,
            text.trim()
        );
    }
}

/// Extract `T` from `Option<T>`
fn unwrap_option_type(ty: Type) -> Type {
    let unwrapped_type = match &ty {
        Type::Path(tp) => {
            let segment = tp.path.segments.last().expect("Invalid type");

            if segment.ident != "Option" {
                panic!("Fallback fields must be of type Option<T>");
            }

            match &segment.arguments {
                PathArguments::AngleBracketed(args) => match args.args.first() {
                    Some(GenericArgument::Type(inner)) => inner.clone(),
                    _ => panic!("Option must have exactly one type parameter"),
                },
                _ => panic!("Option must have type parameters"),
            }
        }
        _ => panic!("Fallback fields must be of type Option<T>"),
    };
    unwrapped_type
}

fn parse_fallback_expr(attr: &Attribute) -> Option<Expr> {
    attr.parse_args::<Expr>().ok()
}
