use std::collections::HashMap;

use heck::ToUpperCamelCase;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Literal, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    Attribute, Expr, ExprParen, ExprType, ItemStruct, Type, TypeArray, TypeTuple, Visibility,
};

enum FieldTypes<'a> {
    Once(&'a Type, &'a Expr),
    Tuple(Vec<(&'a Type, &'a Expr)>),
    Array(Box<FieldTypes<'a>>, &'a Expr),
}

struct Services<'a> {
    id: usize,
    ident: &'a Ident,
    upper: Ident,
    ty: FieldTypes<'a>,
}

#[derive(Debug)]
enum NotifTypes<'a> {
    Once(&'a Ident, Option<usize>),
    Array(&'a Ident, Option<usize>),
}

fn parse_attrs(attrs: &[Attribute]) -> HashMap<String, Ident> {
    attrs
        .into_iter()
        .filter_map(|attr| {
            if attr.path.is_ident("notifier") {
                syn::parse2(attr.tokens.clone())
                    .ok()
                    .map(|ExprParen { expr, .. }| *expr)
                    .and_then(|expr: Expr| match expr {
                        Expr::Type(ExprType { expr, ty, .. }) => Some((*expr, *ty)),
                        _ => None,
                    })
                    .and_then(|(expr, ty)| match expr {
                        Expr::Path(path) => match (path.path.get_ident(), ty) {
                            (Some(ident), Type::Path(path)) => path
                                .path
                                .get_ident()
                                .cloned()
                                .map(|ty| (ident.to_string(), ty)),
                            _ => None,
                        },
                        _ => None,
                    })
            } else {
                None
            }
        })
        .collect()
}

fn parse_field(ty: &Type) -> Result<FieldTypes, syn::Error> {
    match ty {
        Type::Path(ty) => ty
            .path
            .segments
            .last()
            .ok_or((quote!(#ty), "Неправильный путь к типу"))
            .and_then(|segm| match &segm.arguments {
                syn::PathArguments::AngleBracketed(args) if args.args.len() == 2 => {
                    Ok((args.args.first().unwrap(), args.args.last().unwrap()))
                }
                _ => Err((
                    quote!(#segm),
                    "Тип должен содержать два шаблонных аргумента",
                )),
            })
            .and_then(|(ty, num)| match ty {
                syn::GenericArgument::Type(ty) => match num {
                    syn::GenericArgument::Const(num) => Ok((ty, num)),
                    _ => Err((
                        quote!(#num),
                        "Второй тип должен быть константным выражением",
                    )),
                },
                _ => Err((quote!(#ty), "Первый аргумент должен быть типом")),
            })
            .map(|(ty, num)| FieldTypes::Once(ty, num)),
        Type::Tuple(TypeTuple { elems, .. }) => {
            let mut elems = elems.iter();
            let mut ret = Vec::new();
            loop {
                if let Some(ty) = elems.next() {
                    match parse_field(ty) {
                        Ok(FieldTypes::Once(ty, expr)) => ret.push((ty, expr)),
                        Ok(_) => break Err((quote!(#ty), "Сервисы могут быть сруппированны только из серсисов, а не других групп или массивов")),
                        Err(err) => return Err(err),
                    }
                } else {
                    break Ok(FieldTypes::Tuple(ret));
                }
            }
        }
        Type::Array(TypeArray { elem, len, .. }) => match parse_field(elem.as_ref()) {
            Ok(ty) => Ok(FieldTypes::Array(Box::new(ty), len)),
            Err(err) => return Err(err),
        }
        _ => Err((quote!(#ty), "Неподдерживаемый тип")),
    }
    .map_err(|(tokens, msg)| syn::Error::new_spanned(tokens, msg))
}

fn parse(input: &ItemStruct) -> Result<Vec<Services>, TokenStream2> {
    let mut parsed = Vec::with_capacity(input.fields.len());
    for (id, ident, res) in input
        .fields
        .iter()
        .filter(|field| {
            field
                .attrs
                .iter()
                .find(|attr| {
                    attr.path
                        .get_ident()
                        .map_or(false, |ident| ident.to_string() == "service")
                })
                .is_some()
        })
        .enumerate()
        .map(|(id, field)| (id, field.ident.as_ref().unwrap(), parse_field(&field.ty)))
    {
        match res {
            Ok(ty) => parsed.push(Services {
                id,
                ident,
                upper: Ident::new(&ident.to_string().to_uppercase(), ident.span()),
                ty,
            }),
            Err(err) => return Err(err.into_compile_error()),
        }
    }
    Ok(parsed)
}

fn targets(
    vis: &Visibility,
    crate_path: &TokenStream2,
    target: &Ident,
    servs: &Vec<Services>,
) -> TokenStream2 {
    let mut output = TokenStream2::new();

    let r#enum = servs.iter().fold(
        TokenStream2::new(),
        |mut output, Services { upper, ty, .. }| {
            match ty {
                FieldTypes::Once(_, _) | FieldTypes::Tuple(_) => output.extend(quote!(#upper,)),
                FieldTypes::Array(_, _) => output.extend(quote!(#upper (Option<usize>),)),
            };
            output
        },
    );
    output.extend(quote!(
        #[derive(Debug, Clone, Copy)]
        #vis enum #target {
            #r#enum
            GLOBAL_SENDER,
        }
    ));

    let r#impl = servs.iter().fold(
        TokenStream2::new(),
        |mut output, Services { upper, id, ty, .. }| {
            let upper = if matches!(ty, FieldTypes::Array(_, _)) {
                quote!(#upper (_))
            } else {
                quote!(#upper)
            };
            output.extend(quote!(Self::#upper => #id,));
            output
        },
    );
    output.extend(quote!(
        impl #target {
            pub const fn id(&self) -> usize {
                match self {
                    #r#impl
                    Self::GLOBAL_SENDER => usize::MAX,
                }
            }
        }
    ));

    let from_id = servs.iter().fold(
        TokenStream2::new(),
        |mut output, Services { upper, id, ty, .. }| {
            output.extend(if matches!(ty, FieldTypes::Array(_, _)) {
                quote!(#target::#upper (index) => match index {
                    Some(index) => #crate_path::ID::new(#id).set_index(index),
                    None => #crate_path::ID::new(#id)
                }.set_name(target.into()),)
            } else {
                quote!(#target::#upper => #crate_path::ID::new(#id).set_name(target.into()),)
            });
            output
        },
    );
    output.extend(quote!(
        impl From<#target> for #crate_path::ID {
            fn from(target: #target) -> Self {
                match target {
                    #from_id
                    #target ::GLOBAL_SENDER => #crate_path::ID::new(usize::MAX).set_name(target.into())
                }
            }
        }
    ));

    let from_usize = servs.iter().fold(
        TokenStream2::new(),
        |mut output, Services { upper, id, ty, .. }| {
            output.extend(if matches!(ty, FieldTypes::Array(_, _)) {
                quote!(#id => Self::#upper (None),)
            } else {
                quote!(#id => Self::#upper,)
            });
            output
        },
    );
    output.extend(quote!(
        impl From<usize> for #target {
            fn from(id: usize) -> Self {
                match id {
                    #from_usize
                    _ => Self::GLOBAL_SENDER,
                }
            }
        }
    ));

    let from_str = servs.iter().fold(
        TokenStream2::new(),
        |mut output, Services { upper, ty, .. }| {
            let name = syn::LitStr::new(&upper.to_string().to_upper_camel_case(), upper.span());
            let upper = if matches!(ty, FieldTypes::Array(_, _)) {
                quote!(#upper (_))
            } else {
                quote!(#upper)
            };
            output.extend(quote!(#target ::#upper => #name,));
            output
        },
    );
    output.extend(quote!(
        impl From<#target> for &'static str {
            fn from(target: #target) -> Self {
                match target {
                    #from_str
                    #target ::GLOBAL_SENDER => "Global"
                }
            }
        }
    ));

    output
}

fn channel_get(
    name: &Ident,
    crate_path: &TokenStream2,
    target: &Ident,
    servs: &Vec<Services>,
) -> TokenStream2 {
    let mut output = TokenStream2::new();

    servs
        .iter()
        .filter(|Services { ty, .. }| matches!(ty, FieldTypes::Array(_, _)))
        .for_each(|Services { upper, ident, ty, .. }| {
            let (count, (preffix, suffix)) = match ty {
                FieldTypes::Array(ty, ret) => (ret, match ty.as_ref() {
                    FieldTypes::Tuple(_) => (quote!(&), quote!(.0)),
                    _ => (quote!(), quote!()),
                }),
                _ => todo!(),
            };
            output.extend(quote!(
                impl #crate_path::ChannelGet<{ #target::#upper (None).id() }, {#count}> for #name {
                    fn get(&self, index: usize) -> &dyn #crate_path::DynamicServiceId {
                        #preffix self.#ident.get(index).expect(#crate_path::INCORRECT_INDEX)#suffix
                    }
                }
            ))
        });

    output
}

fn service_get(
    name: &Ident,
    crate_path: &TokenStream2,
    target: &Ident,
    servs: &Vec<Services>,
) -> TokenStream2 {
    let mut output = TokenStream2::new();

    servs.iter().for_each(|Services { upper, ident, ty, .. }| {
        let make = |
            id: &TokenStream2, 
            ty: &Type, 
            index: &TokenStream2, 
            preffix: &TokenStream2, 
            suffix: &TokenStream2
        | -> TokenStream2 {
            quote!(
                impl #crate_path::ServiceGet<{ #target::#id.id() }, #ty> for #name {
                    fn get(&self, #index: Option<usize>) -> &'_ dyn #crate_path::DynamicService<#ty> {
                        #preffix self.#ident #suffix
                    }
                }
            )
        };

        let make = |
            ty: &FieldTypes, 
            id: &TokenStream2, 
            _index: &TokenStream2,
            preffix: &TokenStream2, 
            suffix: &TokenStream2
        | {
            match ty {
                FieldTypes::Once(ty, _) => {
                    make(id, ty, _index, preffix, suffix)
                },
                FieldTypes::Tuple(vec) => vec.into_iter().enumerate().fold(TokenStream2::new(), |mut output, (index, (ty, _))| {
                    let index = Literal::usize_unsuffixed(index);
                    output.extend(
                    make(id, ty, _index, &quote!(&), &quote!(#suffix.#index))
                    );
                    output
                }),
                FieldTypes::Array(_, _) => todo!(),
            }
        };

        output.extend(match ty {
            FieldTypes::Array(ty, _) => match ty.as_ref() {
                FieldTypes::Once(_, _) | FieldTypes::Tuple(_) => make(ty, &quote!(#upper (None)), &quote!(index), &quote!(), &quote!(
                    .get(index.expect(#crate_path::INCORRECT_INDEX)).expect(#crate_path::INCORRECT_INDEX)
                )),
                FieldTypes::Array(_, _) => todo!(),
            },
            _ => make(ty, &quote!(#upper), &quote!(_), &quote!(&), &quote!())
        })
    });

    output
}

fn notifier_senders(
    name: &Ident,
    crate_path: &TokenStream2,
    servs: &Vec<Services>
) -> TokenStream2 {
        fn insert<'a>(
            map: &mut HashMap<&'a Type, Vec<NotifTypes<'a>>>,
            ty: &'a Type,
            ident: NotifTypes<'a>,
        ) {
            match map.get_mut(ty) {
                Some(vec) => vec.push(ident),
                None => {
                    map.insert(ty, vec![ident]);
                }
            }
        }

        let sorted = servs.iter().fold(
            HashMap::<&Type, Vec<NotifTypes>>::new(),
            |mut map, Services { ident, ty, .. }| {
                fn wrap<'a>(
                    map: &mut HashMap<&'a Type, Vec<NotifTypes<'a>>>,
                    ty: &'a FieldTypes,
                    ident: &'a Ident,
                    arr: bool,
                ) {
                    let wrap = |ident, index| {
                        if arr {
                            NotifTypes::Array(ident, index)
                        } else {
                            NotifTypes::Once(ident, index)
                        }
                    };
                    match ty {
                        FieldTypes::Once(ty, _) => insert(map, ty, wrap(ident, None)),
                        FieldTypes::Tuple(vec) => vec
                            .into_iter()
                            .enumerate()
                            .for_each(|(index, (ty, _))| insert(map, ty, wrap(ident, Some(index)))),
                        FieldTypes::Array(_, _) => todo!(),
                    }
                }

                match ty {
                    FieldTypes::Array(ty, _) => wrap(&mut map, ty, ident, true),
                    _ => wrap(&mut map, ty, ident, false),
                }
                map
            },
        );
        
        sorted.into_iter().fold(TokenStream2::new(), |mut output, (ty, vec)| {
            let iters = vec.into_iter().enumerate().fold(TokenStream2::new(), |mut output, (index, notif)| {
                let r#as = quote!( as &dyn #crate_path::DynamicService<#ty>);
                let preffix = match &notif {
                    NotifTypes::Once(_, Some(_)) | NotifTypes::Array(_, Some(_)) => {
                        quote!(&)
                    },
                    _ => quote!()
                };
                let suffix = match &notif {
                    NotifTypes::Once(_, Some(index)) | NotifTypes::Array(_, Some(index)) => {
                        let index = Literal::usize_unsuffixed(*index);
                        quote!(.#index)
                    },
                    _ => quote!()
                };
                let ret = match notif {
                    NotifTypes::Once(ident, _) => quote!([&self.#ident #suffix #r#as].into_iter()),
                    NotifTypes::Array(ident, _) => quote!(self.#ident.iter()
                        .map(|item| #preffix item #suffix #r#as)
                    ),
                };
                output.extend(if index != 0 {
                    quote!(.chain(#ret))
                } else {
                    ret
                });
                output
            });
            output.extend(quote!(
                impl #crate_path::NotifierSenders<#ty> for #name {
                    type Iter<'ch> = impl Iterator<Item = &'ch dyn DynamicService<#ty>> + Clone 
                        where 
                            #ty: 'ch,
                            Self: 'ch;
                    fn get<'ch>(&'ch self) -> Self::Iter<'ch> {
                        #iters
                    }
                }
            ));
            output
        })
    
}

fn notifier_impl(input: &ItemStruct) -> TokenStream2 {
    let crate_path = quote!(::target_notifier);

    let name = &input.ident;
    let vis = &input.vis;
    let attrs = parse_attrs(&input.attrs);

    if !matches!(&input.fields, syn::Fields::Named(_)) {
        panic!("{name} should be structure with named fields")
    };
    if attrs.get("targets").is_none() {
        panic!("Attrribute \"targets\" not found")
    }

    let parsed = match parse(input) {
        Ok(res) => res,
        Err(err) => return err,
    };
    let target = attrs.get("targets").unwrap();

    let targets = targets(vis, &crate_path, target, &parsed);
    let channel_get = channel_get(name, &crate_path, target, &parsed);
    let service_get = service_get(name, &crate_path, target, &parsed);
    let notifier_senders = notifier_senders(name, &crate_path, &parsed);
    let notifier = {
        quote!(
            impl #crate_path ::Notifier for #name {}
        )
    };
    let aliases = {
        let mut output = TokenStream2::new();

        let alias = Ident::new(&(name.to_string()+"Channel"), name.span());
        output.extend(quote!(#vis type #alias <'a, const I: usize> = #crate_path::Channel<'a, I, #name, #target>;));
        let alias = Ident::new(&(name.to_string()+"Channels"), name.span());
        output.extend(quote!(#vis type #alias <'a, const I: usize, const U: usize> = #crate_path::Channels<'a, I, U, #name, #target>;));
        let alias = Ident::new(&(name.to_string()+"Sender"), name.span());
        output.extend(quote!(#vis type #alias <'a> = #crate_path::Sender<'a, #name>;));
        let alias = Ident::new(&(name.to_string()+"Receiver"), name.span());
        output.extend(quote!(#vis type #alias <'a, T> = #crate_path::Receiver<'a, T>;));

        output
    };

    let r#impl = {
        let mut output = TokenStream2::new();

        parsed.iter().for_each(|Services { ident, upper, ty, .. }| {
            output.extend(match ty {
                FieldTypes::Once(_, _) | FieldTypes::Tuple(_) => quote!(
                    pub fn #ident(&self) -> #crate_path::Channel<'_, { #target::#upper.id() }, Self, #target> {
                        self.into()
                    }
                ),
                FieldTypes::Array(_, expr) => quote!(
                    pub fn #ident(&self) -> #crate_path::Channels<'_, { #target::#upper(None).id() }, { #expr }, Self, #target> {
                        #crate_path::Channels::new(&self)
                    }
                ),
            });
        });
        let init = parsed.iter().fold(TokenStream2::new(), |mut output, Services { ident, upper, ty, .. }| {
            output.extend(match ty {
                FieldTypes::Once(_, _) => quote!(self.#ident.init(#target::#upper);),
                FieldTypes::Tuple(vec) => {
                    vec.iter().enumerate().fold(TokenStream2::new(), |mut output, (index, _)| {
                        let index = Literal::usize_unsuffixed(index);
                        output.extend(quote!(
                            self.#ident.#index.init(#target::#upper);
                        ));
                        output
                    })
                },
                FieldTypes::Array(ty, _) => {
                    let body = match ty.as_ref() {
                        FieldTypes::Once(_, _) => quote!(#ident.init(id)),
                        FieldTypes::Tuple(vec) => {
                            vec.iter().enumerate().fold(TokenStream2::new(), |mut output, (index, _)| {
                                let index = Literal::usize_unsuffixed(index);
                                output.extend(quote!(
                                    #ident.#index.init(id);
                                ));
                                output
                            })
                        },
                        FieldTypes::Array(_, _) => todo!(),
                    };
                    quote!(
                        #crate_path::Service::array(#target::#upper(None), &mut self.#ident, |id, #ident| {
                            #body
                        });
                    )
                },
            });
            output
        });
        output.extend(quote!(
            pub fn init_notifier(&mut self) {
                #init
            }
        ));

        quote!(impl #name { #output })
    };

    quote!(
        #aliases
        #targets
        #channel_get
        #service_get
        #notifier_senders
        #notifier
        #r#impl
    )
}

#[proc_macro_derive(Notifier, attributes(service, notifier))]
pub fn macro_body(input: TokenStream) -> TokenStream {
    match syn::parse(input).map(|input: ItemStruct| notifier_impl(&input)) {
        Ok(output) => TokenStream::from(output),
        Err(err) => err.into_compile_error().into(),
    }
}
