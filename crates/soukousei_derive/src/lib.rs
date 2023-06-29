use darling::{FromDeriveInput, FromField, FromMeta};
use proc_macro::TokenStream;
use syn::{parse_macro_input, Expr, Lit};

#[derive(Debug, FromDeriveInput, Eq, PartialEq)]
#[darling(attributes(layer), supports(struct_named))]
struct LayerArgs {
    ident: syn::Ident,
    data: darling::ast::Data<darling::util::Ignored, LayerFieldArgs>,
    // TODO: how to collect all struct-level serde attributes? So that we can pass them to the Partial
    // TODO: inherit visibility?
}

#[derive(Debug, FromField, Eq, PartialEq)]
#[darling(attributes(layer))]
struct LayerFieldArgs {
    ident: Option<syn::Ident>,
    ty: syn::Type,

    /// Associated default value
    default: Option<String>,
    /// Associated ENV var(s)
    env: Option<LayerParamEnv>,
    /// Flag that indicates that there is a nested configuration
    ///
    /// TODO: can we validate that the type of the nested field has `::Partial`?
    #[darling(default)]
    nested: bool,
    // TODO how to collect all field-level serde attributes? So that we can pass them to the Partial
}

trait IsIdentOption {
    fn is_option_already(&self) -> bool;
}

impl IsIdentOption for syn::Ident {
    fn is_option_already(&self) -> bool {
        todo!()
    }
}

struct LayerFieldBase {
    ident: syn::Ident,
    ty: syn::Type,
}

enum LayerField {
    Nested {
        base: LayerFieldBase,
    },
    Field {
        base: LayerFieldBase,
        // TODO: should be not a string, but a parsed expression, like `Default::default()`
        default: Option<String>,
        env: Option<LayerParamEnv>,
    },
}

impl TryFrom<LayerFieldArgs> for LayerField {
    type Error = ();

    fn try_from(
        LayerFieldArgs {
            ident,
            ty,
            default,
            env,
            nested,
        }: LayerFieldArgs,
    ) -> Result<Self, Self::Error> {
        let ident = ident.ok_or(())?;
        let base = LayerFieldBase { ident, ty };
        let param = match (nested, default, env) {
            (true, None, None) => LayerField::Nested { base },
            (false, default, env) => LayerField::Field { base, default, env },
            _ => return Err(()),
        };
        Ok(param)
    }
}

#[derive(Debug, Eq, PartialEq)]
enum LayerParamEnv {
    Single(String),
    Multiple(Vec<String>),
}

impl FromMeta for LayerParamEnv {
    fn from_expr(expr: &Expr) -> darling::Result<Self> {
        use syn::{ExprArray, ExprLit, Lit, LitStr};

        match expr {
            // TODO: is there a less verbose way to parse expr as `["A", "B"]`?
            Expr::Array(ExprArray { attrs, elems, .. }) if attrs.len() == 0 && elems.len() > 0 => {
                let literals = elems
                    .into_iter()
                    .map(|lit_expr| match lit_expr {
                        Expr::Lit(ExprLit {
                            attrs,
                            lit: Lit::Str(lit),
                        }) if attrs.len() == 0 => Ok(lit.value()),
                        _ => Err(darling::Error::unexpected_expr_type(lit_expr)),
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Self::Multiple(literals))
            }
            Expr::Lit(ExprLit {
                attrs,
                lit: Lit::Str(lit),
            }) if attrs.len() == 0 => Ok(Self::Single(lit.value())),
            _ => Err(darling::Error::unexpected_expr_type(expr)),
        }
    }
}

mod codegen {
    use super::LayerField;
    use super::LayerParamEnv;
    use crate::LayerArgs;
    use darling::FromMeta;
    use miette::{miette, Result};
    use proc_macro2::TokenStream;
    use quote::quote;

    struct Ir {
        ident_main: syn::Ident,
        ident_layer: syn::Ident,
        impl_default: bool,
        impl_from_env: bool,
        fields: Vec<IrField>,
    }

    enum IrField {
        Plain {
            id: syn::Ident,
            ty: syn::Type,
            default: Option<syn::Expr>,
            env: Option<LayerParamEnv>,
            is_optional: bool,
        },
        NestedLayer {
            id: syn::Ident,
            layer_ty: syn::Type,
        },
    }

    impl IrField {
        fn codegen_new(&self) -> TokenStream {
            match self {
                Self::Plain { id, .. } => quote! { #id: None },
                Self::NestedLayer { id, .. } => quote! { #id: ::soukousei::Layer::new() },
            }
        }

        fn codegen_merge(&self) -> TokenStream {
            match self {
                Self::Plain { id, .. } => quote! { #id: other.#id.or(self.#id) },
                Self::NestedLayer { id, .. } => {
                    quote! { #id: ::soukousei::Layer::merge(self.#id, other.#id) }
                }
            }
        }

        fn codegen_default(&self) -> TokenStream {
            match self {
                Self::Plain {
                    id,
                    default: Some(default),
                    ..
                } => quote! { #id: Some(#default) },
                Self::Plain {
                    id,
                    // FIXME: how to match for `None`, and not bind to variable `None`?
                    default: Option::None,
                    ..
                } => quote! { #id: None },
                Self::NestedLayer { id, .. } => quote! { #id: Default::default() },
            }
        }
    }

    impl Ir {
        pub fn from_args(args: LayerArgs) -> Result<Self> {
            let ident_main = args.ident.clone();
            let ident_layer = paste::paste! { [<#ident_main Layer>] };

            let fields = args
                .data
                .take_struct()
                .ok_or_else(|| miette!("not a struct"))?
                .fields
                .into_iter()
                .map(|field_args| todo!());

            todo!()
        }

        pub fn codegen(&self) -> TokenStream {
            let fields_new = self.codegen_new_fields();

            let fields_merge = self.codegen_fields_merge();

            let mut tokens = quote! {
                impl ::soukousei::HasLayer for #self.ident_main {
                    type Layer = #self.ident_layer;
                }

                impl ::soukousei::Layer for #self.ident_layer {
                    type Complete = #self.ident_main;

                    fn new() -> Self {
                        Self {
                            #fields_new
                        }
                    }

                    fn merge(self, other: Self) -> Self {
                        Self {
                            #fields_merge
                        }
                    }

                    fn complete(self) -> Result<Self::Complete, ::soukousei::CompleteError> {
                        // TODO
                    }
                }
            };

            if self.impl_default {
                let fields_default = self.codegen_fields_default();

                tokens.extend(quote! {
                   impl Default for #self.ident_layer {
                        fn default() -> Self {
                            Self {
                                #fields_default
                            }
                        }
                    }
                });
            }

            if self.impl_from_env {
                tokens.extend(quote! {
                    impl ::soukousei::FromEnv for #self.ident_layer {
                        // TODO
                    }
                })
            }

            tokens
        }

        fn codegen_new_fields(&self) -> TokenStream {
            let fields: Vec<_> = self.fields.iter().map(|x| x.codegen_new()).collect();

            quote! {
                #(#fields),*
            }
        }

        fn codegen_fields_merge(&self) -> TokenStream {
            let fields: Vec<_> = self.fields.iter().map(|x| x.codegen_merge()).collect();

            quote! {
                #(#fields),*
            }
        }

        fn codegen_fields_default(&self) -> TokenStream {
            let fields: Vec<_> = self.fields.iter().map(|x| x.codegen_default()).collect();

            quote! {
                #(#fields),*
            }
        }
    }

    impl Ir {
        // fn codegen(self) {
        //     let fields_for_new = self.fields.iter().map(|field| match field {
        //         ConfigField::Field { base, .. } => {
        //             let id = &base.ident;
        //             quote! { #id: None }
        //         }
        //         ConfigField::Nested { base, .. } => {
        //             let id = &base.ident;
        //             let ty = &base.ty;
        //             quote! {}
        //         }
        //     });
        //
        //     let tokens = quote! {
        //         // original struct, no changes
        //
        //
        //         impl #self.ident_main {
        //             pub type Partial = #self.ident_partial;
        //         }
        //
        //         #[derive(Serialize, Deserialize)]
        //         struct #self.ident_partial {
        //             // fields
        //         }
        //
        //         impl ::totonoeru::Partial for #self.ident_partial {
        //             type Resolved = #self.ident_main;
        //
        //             fn new() -> Self {
        //                 Self {
        //
        //                 }
        //             }
        //
        //             fn default() -> Self {
        //                 todo!()
        //             }
        //
        //             fn from_env<P, E>(provider: P) -> Result<Self, E>
        //             where
        //                 Self: Sized,
        //                 P: ::totonoeru::env::EnvProvider<FetchError = E> {
        //                 todo!()
        //             }
        //
        //             fn merge(self, other: Self) -> Self {
        //                 todo!()
        //             }
        //
        //             fn resolve(self) -> Result<Self::Resolved, ::totonoeru::ResolveError> {
        //                 todo!()
        //             }
        //         }
        //     };
        // }
    }
}

#[proc_macro_derive(Layer)]
pub fn derive_layer(_item: TokenStream) -> TokenStream {
    todo!()
}

#[cfg(test)]
mod tests {
    use crate::{LayerArgs, LayerParamEnv};
    use darling::FromDeriveInput;
    use expect_test::expect;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn parse_all_attributes() {
        let input = parse_quote! {
            #[derive(Layer)]
            struct Test {
                #[layer(default = "100")]
                foo: u32,
                bar: Option<String>,
                #[layer(env = "ENV")]
                baz: bool,
                #[layer(env = ["FOO", "BAR"])]
                foo_bar: u32,
                #[layer(nested)]
                nested: AnotherConfig
            }
        };

        let parsed = LayerArgs::from_derive_input(&input).unwrap();

        assert_eq!(parsed.ident.to_string(), "Test");

        let mut fields = parsed.data.take_struct().unwrap().fields.into_iter();

        let foo = fields.next().unwrap();
        assert_eq!(foo.default, Some("100".to_owned()));
        assert_eq!(foo.env, None);
        assert_eq!(foo.nested, false);

        let bar = fields.next().unwrap();
        assert_eq!(bar.default, None);
        assert_eq!(bar.env, None);
        assert_eq!(bar.nested, false);

        let baz = fields.next().unwrap();
        assert_eq!(baz.default, None);
        assert_eq!(baz.env, Some(LayerParamEnv::Single("ENV".to_owned())));
        assert_eq!(baz.nested, false);

        let foo_bar = fields.next().unwrap();
        assert_eq!(foo_bar.default, None);
        assert_eq!(
            foo_bar.env,
            Some(LayerParamEnv::Multiple(
                ["FOO", "BAR"].into_iter().map(ToOwned::to_owned).collect()
            ))
        );
        assert_eq!(foo_bar.nested, false);

        let nested = fields.next().unwrap();
        assert_eq!(nested.nested, true);
    }

    #[test]
    #[should_panic]
    fn nested_with_env_is_not_allowed() {
        todo!()
    }

    #[test]
    #[should_panic]
    fn nested_with_default_is_not_allowed() {
        todo!()
    }
}
