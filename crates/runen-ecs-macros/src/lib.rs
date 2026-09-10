use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::{
    Data, DataEnum, DataStruct, DeriveInput, Fields, GenericArgument, GenericParam, Ident, Type,
    parse_macro_input, parse_quote,
};

fn ecs_crate_path() -> proc_macro2::TokenStream {
    match crate_name("runen-ecs") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote!(::#ident)
        }
        Err(_) => quote!(::runen_ecs),
    }
}

#[proc_macro_derive(Component)]
pub fn component_derive(input: TokenStream) -> TokenStream {
    let ecs = ecs_crate_path();
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    TokenStream::from(quote! {
        impl #impl_generics #ecs::Component for #name #ty_generics #where_clause {}
    })
}

#[proc_macro_derive(Resource)]
pub fn resource_derive(input: TokenStream) -> TokenStream {
    let ecs = ecs_crate_path();
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    TokenStream::from(quote! {
        impl #impl_generics #ecs::Resource for #name #ty_generics #where_clause {}
    })
}

#[proc_macro_derive(SystemParam)]
pub fn system_param_derive(input: TokenStream) -> TokenStream {
    let ecs = ecs_crate_path();
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = input.generics;
    let Data::Struct(data) = input.data else {
        return TokenStream::from(quote! {
            compile_error!("SystemParam derive only supports structs with named fields");
        });
    };
    let Fields::Named(fields) = data.fields else {
        return TokenStream::from(quote! {
            compile_error!("SystemParam derive only supports structs with named fields");
        });
    };
    if fields.named.is_empty() {
        return TokenStream::from(quote! {
            compile_error!("SystemParam derive requires at least one named field");
        });
    }

    for param in &generics.params {
        if let syn::GenericParam::Lifetime(param) = param {
            let lifetime = &param.lifetime;
            let supported = lifetime.ident == "w" || lifetime.ident == "s";
            if !supported {
                return syn::Error::new_spanned(
                    lifetime,
                    "SystemParam derive only supports the lifetime roles 'w (world) and 's (cached state)",
                )
                .to_compile_error()
                .into();
            }
        }
    }

    let fields = fields.named.into_iter().collect::<Vec<_>>();
    let field_idents = fields
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect::<Vec<_>>();
    let field_names = field_idents
        .iter()
        .map(|field_ident| field_ident.to_string())
        .collect::<Vec<_>>();
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let state_indices = (0..fields.len()).map(syn::Index::from).collect::<Vec<_>>();

    let impl_generics = generics.clone();
    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
    let item_args = generics
        .params
        .iter()
        .map(|param| match param {
            syn::GenericParam::Lifetime(param) if param.lifetime.ident == "w" => quote!('world),
            syn::GenericParam::Lifetime(_) => quote!('state),
            syn::GenericParam::Type(param) => {
                let ident = &param.ident;
                quote!(#ident)
            }
            syn::GenericParam::Const(param) => {
                let ident = &param.ident;
                quote!(#ident)
            }
        })
        .collect::<Vec<_>>();
    let (_, ty_generics, _) = generics.split_for_impl();
    let item_type = if item_args.is_empty() {
        quote!(#name)
    } else {
        quote!(#name <#(#item_args),*>)
    };
    let item_ctor = if item_args.is_empty() {
        quote!(#name)
    } else {
        quote!(#name :: <#(#item_args),*>)
    };
    let group_label = name.to_string();

    TokenStream::from(quote! {
        unsafe impl #impl_generics #ecs::SystemParam for #name #ty_generics #where_clause {
            type State = (
                #(<#field_types as #ecs::SystemParam>::State,)*
            );
            type Item<'world, 'state> = #item_type;

            fn init_state(world: &mut #ecs::World) -> Result<Self::State, #ecs::SystemParamError> {
                Ok((
                    #(<#field_types as #ecs::SystemParam>::init_state(world)?,)*
                ))
            }

            fn access(state: &Self::State) -> #ecs::QueryAccess {
                let mut access = #ecs::QueryAccess::default();
                #(
                    access.extend(<#field_types as #ecs::SystemParam>::access(&state.#state_indices));
                )*
                access
            }

            fn slot_descriptor() -> #ecs::ParamSlotDescriptor {
                #ecs::ParamSlotDescriptor::group(
                    "param_group",
                    #group_label,
                    ::std::any::type_name::<Self>(),
                    vec![
                        #(
                            #ecs::ParamSlotDescriptor::named_child(
                                #field_names,
                                <#field_types as #ecs::SystemParam>::slot_descriptor(),
                            ),
                        )*
                    ],
                )
            }

            unsafe fn extract<'world, 'state>(
                state: &'state mut Self::State,
                context: #ecs::SystemParamContext<'world>,
            ) -> Result<Self::Item<'world, 'state>, #ecs::SystemParamError> {
                Ok(#item_ctor {
                    #(
                        #field_idents: unsafe {
                            <#field_types as #ecs::SystemParam>::extract(
                                &mut state.#state_indices,
                                context,
                            )?
                        },
                    )*
                })
            }
        }
    })
}

#[proc_macro_derive(Bundle)]
pub fn bundle_derive(input: TokenStream) -> TokenStream {
    let ecs = ecs_crate_path();
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let Data::Struct(data) = input.data else {
        return TokenStream::from(quote! {
            compile_error!("Bundle derive only supports structs");
        });
    };

    let Fields::Named(fields) = data.fields else {
        return TokenStream::from(quote! {
            compile_error!("Bundle derive only supports structs with named fields");
        });
    };

    let descriptors = fields.named.iter().map(|field| {
        let ty = &field.ty;
        quote! {
            #ecs::BundleComponentDescriptor::of::<#ty>()
        }
    });

    let transfers = fields.named.iter().map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        quote! {
            components.push(self.#field_name);
        }
    });

    let removals = fields.named.iter().map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        quote! {
            #field_name: components
                .take::<#ty>()
                .expect("removed bundle component must match its descriptor")
        }
    });

    TokenStream::from(quote! {
        unsafe impl #impl_generics #ecs::Bundle for #name #ty_generics #where_clause {
            fn __component_descriptors() -> ::std::vec::Vec<#ecs::BundleComponentDescriptor> {
                ::std::vec![#(#descriptors),*]
            }

            fn __into_components(self, components: &mut #ecs::BundleComponents) {
                #(#transfers)*
            }

            unsafe fn __from_components(components: &mut #ecs::BundleComponents) -> Self {
                Self {
                    #(#removals,)*
                }
            }
        }
    })
}

#[proc_macro_derive(Reflect)]
pub fn reflect_derive(input: TokenStream) -> TokenStream {
    expand_reflect(input)
}

fn expand_reflect(input: TokenStream) -> TokenStream {
    let ecs = ecs_crate_path();
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = input.generics;

    if generics
        .params
        .iter()
        .any(|param| matches!(param, GenericParam::Lifetime(_)))
    {
        return TokenStream::from(quote! {
            compile_error!("Reflect derive does not support borrowed lifetime parameters; reflected values must be 'static");
        });
    }

    match input.data {
        Data::Struct(data) => expand_reflect_struct(ecs, name, generics, data),
        Data::Enum(data) => expand_reflect_enum(ecs, name, generics, data),
        Data::Union(_) => TokenStream::from(quote! {
            compile_error!("Reflect derive does not support unions; use a named struct or unit enum");
        }),
    }
}

fn add_static_and_field_bounds(
    ecs: &proc_macro2::TokenStream,
    mut generics: syn::Generics,
    field_types: &[&Type],
) -> syn::Generics {
    let type_params = generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect::<Vec<_>>();
    let where_clause = generics.make_where_clause();
    for field_ty in field_types {
        where_clause
            .predicates
            .push(parse_quote!(#field_ty: #ecs::reflect::Reflect));
    }
    for ident in type_params {
        where_clause.predicates.push(parse_quote!(#ident: 'static));
    }
    generics
}

fn contains_reference(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) => true,
        Type::Array(array) => contains_reference(&array.elem),
        Type::Slice(slice) => contains_reference(&slice.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(contains_reference),
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return false;
            };
            arguments.args.iter().any(|argument| match argument {
                GenericArgument::Type(ty) => contains_reference(ty),
                _ => false,
            })
        }),
        _ => false,
    }
}

fn expand_reflect_struct(
    ecs: proc_macro2::TokenStream,
    name: Ident,
    generics: syn::Generics,
    data: DataStruct,
) -> TokenStream {
    let Fields::Named(fields) = data.fields else {
        return TokenStream::from(quote! {
            compile_error!("Reflect derives currently support only named structs");
        });
    };

    let field_types = fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    if field_types.iter().any(|ty| contains_reference(ty)) {
        return TokenStream::from(quote! {
            compile_error!("Reflect derive does not support borrowed fields; reflected values must be 'static");
        });
    }

    let generics = add_static_and_field_bounds(&ecs, generics, &field_types);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_count = fields.named.len();
    let field_arms = fields.named.iter().enumerate().map(|(index, field)| {
        let field_ident = field.ident.as_ref().expect("named field");
        let field_name_string = field_ident.to_string();
        let field_ty = &field.ty;

        quote! {
            #index => {
                let get_ref: #ecs::reflect::FieldGetRef = |owner| {
                    let typed = owner.downcast_ref::<Self>()?;
                    Some(#ecs::reflect::ReflectValueRef::new(&typed.#field_ident))
                };
                let get_mut: #ecs::reflect::FieldGetMut = |owner| {
                    let typed = owner.downcast_mut::<Self>()?;
                    Some(#ecs::reflect::ReflectValueMut::new(&mut typed.#field_ident))
                };
                Some(#ecs::reflect::FieldInfo::new(
                    #field_name_string,
                    #field_name_string,
                    || <#field_ty as #ecs::reflect::Reflect>::type_info(),
                    get_ref,
                    get_mut,
                ))
            }
        }
    });

    TokenStream::from(quote! {
        impl #impl_generics #ecs::reflect::Reflect for #name #ty_generics #where_clause {
            fn type_info() -> #ecs::reflect::TypeInfo
            where
                Self: Sized,
            {
                #ecs::reflect::TypeInfo::new(
                    ::std::any::type_name::<Self>(),
                    stringify!(#name),
                    #ecs::reflect::ReflectShape::Struct(
                        #ecs::reflect::StructInfo::new(
                            #field_count,
                            |index| match index {
                                #(#field_arms,)*
                                _ => None,
                            },
                        ),
                    ),
                )
            }
        }
    })
}

fn expand_reflect_enum(
    ecs: proc_macro2::TokenStream,
    name: Ident,
    generics: syn::Generics,
    data: DataEnum,
) -> TokenStream {
    let mut variant_idents = Vec::new();
    let mut variant_symbols = Vec::new();
    for variant in data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return TokenStream::from(quote! {
                compile_error!("Reflect enum derives currently support only unit/no-payload variants");
            });
        }
        variant_symbols.push(variant.ident.to_string());
        variant_idents.push(variant.ident);
    }

    if variant_idents.is_empty() {
        return TokenStream::from(quote! {
            compile_error!("Reflect enum derives require at least one unit variant");
        });
    }

    let current_arms = variant_idents
        .iter()
        .zip(variant_symbols.iter())
        .map(|(ident, symbol)| {
            quote! {
                Self::#ident => Some(#symbol),
            }
        });

    let set_arms = variant_idents
        .iter()
        .zip(variant_symbols.iter())
        .map(|(ident, symbol)| {
            quote! {
                #symbol => {
                    *typed = Self::#ident;
                    true
                }
            }
        });

    let variant_arms = variant_symbols.iter().enumerate().map(|(index, symbol)| {
        quote! {
            #index => Some(#ecs::reflect::EnumVariantInfo::new(#symbol, #symbol)),
        }
    });
    let generics = add_static_and_field_bounds(&ecs, generics, &[]);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let variant_count = variant_idents.len();

    TokenStream::from(quote! {
        impl #impl_generics #ecs::reflect::Reflect for #name #ty_generics #where_clause {
            fn type_info() -> #ecs::reflect::TypeInfo
            where
                Self: Sized,
            {
                let current_variant: #ecs::reflect::EnumCurrentVariant = |owner| {
                    let typed = owner.downcast_ref::<Self>()?;
                    match typed {
                        #(#current_arms)*
                    }
                };

                let set_unit_variant: #ecs::reflect::EnumSetUnitVariant = |owner, symbol| {
                    let Some(typed) = owner.downcast_mut::<Self>() else {
                        return false;
                    };
                    match symbol {
                        #(#set_arms)*
                        _ => false,
                    }
                };

                #ecs::reflect::TypeInfo::new(
                    ::std::any::type_name::<Self>(),
                    stringify!(#name),
                    #ecs::reflect::ReflectShape::Enum(
                        #ecs::reflect::EnumInfo::new(
                            #variant_count,
                            |index| match index {
                                #(#variant_arms)*
                                _ => None,
                            },
                            current_variant,
                            set_unit_variant,
                        ),
                    ),
                )
            }
        }
    })
}
