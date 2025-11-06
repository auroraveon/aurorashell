use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Expr, Ident, Path, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

/// call this once to create the necessary boilerplate for the module
///
/// using this macro more than once will result in a compile error
///
/// note: add example usage code
#[proc_macro]
pub fn create_module(input: TokenStream) -> TokenStream {
    let CreateModuleArgs {
        module_ident,
        new_fn,
        update_fn,
        view_fn,
        message_ident,
    } = parse_macro_input!(input as CreateModuleArgs);

    let expanded = quote! {
        static STATE: std::sync::LazyLock<std::sync::Mutex<Option<Box<#module_ident>>>> =
            std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

        #[unsafe(no_mangle)]
        fn setup() -> *const ::aurorashell_module::setup::SetupFuncData {
            let (module, setup_data): (#module_ident, ::aurorashell_module::setup::SetupData) = #new_fn();

            *STATE.lock().unwrap() = Some(Box::new(module));

            setup_data.into()
        }

        #[unsafe(no_mangle)]
        fn update(id: u32, data_ptr: u32) -> u32 {
            let mut guard = STATE.lock().expect("state lock poisoned");
            let mut state = match &mut *guard {
                Some(state) => state,
                None => return 0,
            };

            let message: #message_ident = match #message_ident::try_from(id, data_ptr) {
                Ok(msg) => msg,
                Err(err) => {
                    eprintln!("{}", err);
                    return 0;
                }
            };

            let result = #update_fn(&mut state, message);
            if let Some(message) = result {
                message.into()
            } else {
                0
            }
        }

        #[unsafe(no_mangle)]
        fn view(id: u32) -> *const ::aurorashell_module::ViewFuncData {
            let guard = STATE.lock().expect("state lock poisoned");
            let state = match &*guard {
                Some(state) => state,
                None => return std::ptr::null() as *const ::aurorashell_module::ViewFuncData,
            };

            let element = #view_fn(&state, id);

            ::aurorashell_module::view_build_ui(element, id)
        }
    };

    TokenStream::from(expanded)
}

struct CreateModuleArgs {
    module_ident: Ident,
    new_fn: Path,
    update_fn: Path,
    view_fn: Path,
    message_ident: Ident,
}

impl Parse for CreateModuleArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let module_ident: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let new_fn: Path = input.parse()?;
        input.parse::<Token![,]>()?;
        let update_fn: Path = input.parse()?;
        input.parse::<Token![,]>()?;
        let view_fn: Path = input.parse()?;
        input.parse::<Token![,]>()?;
        let message_ident: Ident = input.parse()?;
        input.parse::<Token![,]>()?;

        Ok(CreateModuleArgs {
            module_ident,
            new_fn,
            update_fn,
            view_fn,
            message_ident,
        })
    }
}

////////////////////////////////////////////////////////////////////////////////

/// A procedural macro that creates a `Registers` collection with compile-time duplicate detection.
///
/// # Example
/// ```rust
/// let regs = registers![
///     Interval::from_millis(1000),
///     Interval::from_millis(2000),  // OK - Interval allows duplicates
///     PulseAudio::DEFAULT_SINK,     // OK - first PulseAudio
///     PulseAudio::SINKS,            // Would fail - PulseAudio doesn't allow duplicates
/// ];
/// ```
#[proc_macro]
pub fn registers(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RegistersInput);

    let registers: Vec<_> = input.registers.iter().collect();

    if registers.is_empty() {
        return TokenStream::from(quote! {
            ::aurorashell_module::register::Registers::new()
        });
    }

    let type_paths: Vec<_> = registers
        .iter()
        .map(|register| {
            // gets either the type from before the `:` or just the expression
            // if it wasn't annotated with a type
            if let Some(ref type_path) = register.register_type {
                type_path.clone()
            } else {
                extract_type_path(&register.expression)
            }
        })
        .collect();

    let expressions: Vec<_> = registers
        .iter()
        .map(|register| register.expression.clone())
        .collect();

    let expanded = quote! {
        {
            use ::aurorashell_module::register::IntoRegister;

            // compile-time duplicate detection using const evaluation
            const _: () = {
                const fn check_duplicates<const N: usize>(ids: [u16; N], allows_dups: [bool; N]) {
                    let mut i = 0;
                    while i < N {
                        if !allows_dups[i] {
                            let mut j = i + 1;
                            while j < N {
                                if ids[i] == ids[j] {
                                    panic!("duplicate register id detected");
                                }
                                j += 1;
                            }
                        }
                        i += 1;
                    }
                }

                // build the arrays with the ids and duplicate flags
                check_duplicates(
                    [#(#type_paths::const_id(),)*],
                    [#(#type_paths::const_allow_duplicates(),)*]
                );
            };

            // take the expressions and put them into a vec
            let out = vec![#(
                (#expressions).into_register(),
            )*];
            ::aurorashell_module::register::Registers::from_macro(out)
        }
    };

    TokenStream::from(expanded)
}

/// parses the input to the procedural macro
///
/// ```rust
/// let interval = Interval: Interval::from_millis(2000);
///
/// registers![
///     Interval::from_millis(1000),
///     Interval: interval,
///     PulseAudio: PulseAudio::SINKS | PulseAudio::SOURCES,
/// ];
/// ```
struct RegistersInput {
    registers: Punctuated<RegisterEntry, Token![,]>,
}

impl syn::parse::Parse for RegistersInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let registers = input.parse_terminated(RegisterEntry::parse, Token![,])?;
        Ok(RegistersInput { registers })
    }
}

/// can parse either:
///
/// ```rust
/// Interval::from_millis(1000),
/// Interval: Interval::from_millis(1000),
/// ```
struct RegisterEntry {
    /// represents the part before the `:` in the annotated variant
    ///
    /// ```rust
    /// Interval: Interval::from_millis(1000),
    /// ```
    ///
    /// if this is `None`, then the syntax would be inferred:
    ///
    /// ```rust
    /// Interval::from_millis(1000),
    /// ```
    register_type: Option<Path>,
    /// this references the part after the `:` and before the `,` at the end
    /// of the element:
    ///
    /// ```rust
    /// Interval::from_millis(1000),
    /// ```
    expression: Expr,
}

impl Parse for RegisterEntry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // try to parse as type: expression first
        // we need to fork the input to look ahead without consuming tokens
        let fork = input.fork();

        // try to parse a type path
        if let Ok(Type::Path(_)) = fork.parse::<Type>() {
            // check if the next token is specifically a colon (not ::)
            if fork.peek(Token![:]) && !fork.peek(Token![::]) {
                // this branch is for expressions like:
                // ```rust
                // Interval: Interval::from_millis(1000),
                // ```
                let register_type = match input.parse::<Type>() {
                    Ok(Type::Path(type_path)) => Some(type_path.path),
                    _ => return Err(input.error("expected a type path")),
                };
                let _colon: Token![:] = input.parse()?;
                let expression = input.parse()?;
                Ok(RegisterEntry {
                    register_type,
                    expression,
                })
            } else {
                // this branch is for expressions like:
                // ```rust
                // Interval::from_millis(1000),
                // ```
                let expression = input.parse()?;
                Ok(RegisterEntry {
                    register_type: None,
                    expression,
                })
            }
        } else {
            // couldn't parse as type path, so it must be just an expression
            let expression = input.parse()?;
            Ok(RegisterEntry {
                register_type: None,
                expression,
            })
        }
    }
}

/// extract the type path from a register expression
/// this handles patterns like:
/// - Interval::from_millis(1000) -> Interval
/// - PulseAudio::DEFAULT_SINK -> PulseAudio
/// - SomeRegister::new() -> SomeRegister
/// - MyStruct { field: value } -> MyStruct
fn extract_type_path(expr: &Expr) -> Path {
    match expr {
        // handle Foo::method() calls
        Expr::Call(expr) => {
            if let Expr::Path(path_expr) = &*expr.func {
                // for calls like Interval::from_millis(), we want just "Interval"
                let segments = &path_expr.path.segments;
                if segments.len() >= 2 {
                    // take all segments except the last one (which is the method name)
                    let mut type_path = syn::Path {
                        leading_colon: path_expr.path.leading_colon,
                        segments: syn::punctuated::Punctuated::new(),
                    };
                    // take all but the last segment
                    for segment in segments.iter().take(segments.len() - 1) {
                        type_path.segments.push(segment.clone());
                    }
                    return type_path;
                } else if segments.len() == 1 {
                    // single segment call like foo(), treat as the type
                    return path_expr.path.clone();
                }
            }
        }

        // handle Foo::CONSTANT paths (like PulseAudio::DEFAULT_SINK)
        Expr::Path(expr) => {
            let segments = &expr.path.segments;
            if segments.len() >= 2 {
                // for paths like PulseAudio::DEFAULT_SINK, we want just "PulseAudio"
                let mut type_path = syn::Path {
                    leading_colon: expr.path.leading_colon,
                    segments: syn::punctuated::Punctuated::new(),
                };
                // take only the first segment (the type name)
                if let Some(first_segment) = segments.first() {
                    type_path.segments.push(first_segment.clone());
                }
                return type_path;
            } else {
                // single segment path, use as-is
                return expr.path.clone();
            }
        }

        // handle Foo {} struct literals
        Expr::Struct(expr) => {
            return expr.path.clone();
        }

        Expr::Binary(expr) => match &*expr.left {
            Expr::Path(expr) => {
                let segments = &expr.path.segments;
                if segments.len() >= 2 {
                    // for paths like PulseAudio::DEFAULT_SINK, we want just "PulseAudio"
                    let mut type_path = syn::Path {
                        leading_colon: expr.path.leading_colon,
                        segments: syn::punctuated::Punctuated::new(),
                    };
                    // take only the first segment (the type name)
                    if let Some(first_segment) = segments.first() {
                        type_path.segments.push(first_segment.clone());
                    }
                    return type_path;
                } else {
                    // single segment path, use as-is
                    return expr.path.clone();
                }
            }
            expr => {
                panic!("invalid expression: {:?}", expr)
            }
        },

        expr => {
            panic!("invalid expression: {:?}", expr)
        }
    }

    // fallback: create a dummy path - this should rarely happen
    syn::parse_str("UnknownType").unwrap()
}
