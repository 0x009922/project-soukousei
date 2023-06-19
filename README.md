# Project soukousei (??) (wip)

> 層構成「そう・こう・せい」(soukousei) - layer composition
> 
> IDK how to name this project
> 
> It is more about "partials" and "layers" than about "configuration". Maybe the concept `Partial` should be renamed to `Layer`, as it is very similar to drawing composition: having multiple layers with different parts of the picture, you compose them into a complete picture.

The primary intention of the library is to make configuration simple and reliable. It gives a way to compose a struct from multiple layers in a customisable, modular and non-boilerplate way. Main construction ways are deserialization (`serde`) and ENV loading[^1]. All of this should be covered with great error reporting, because the configuration part usually is a part of public API.

> The result so far is that the library consists **mostly** from usual fine-tuned Rust code, and from a macro that will generate boilerplate code.
> 
> You can see a very draft in [`manual_macro.rs`](./crates/soukousei/tests/manual_macro.rs) where I have manually written the code that the macro will generate. When the general library design is established, it will make sense to actually implement the macro.

## Design

This library has (wip) a macro to generate a partial of a struct. Let's call it `Partial`[^2]:

```rust
use std::num::NonZeroU64;
use soukousei::Partial;

#[derive(Partial)]
struct Config {
    #[param(default = "200")] // foo: 100
    foo: u32,
    #[param(default)]         // bar: Default::default()
    bar: bool,
    #[param(env = "BAZ")]
    baz: String,

    // multiple envs, default fallback
    #[param(env = ["MY_APP_LOG_LEVEL", "LOG_LEVEL"], default)]
    log_level: LogLevel,
    
    // might remain an option even after the resolution
    optional_param: Option<u64>,
    
    #[param(partial)]
    nested: Nested
}

#[derive(Partial)]
struct Nested {
    #[param(default = "DEFAULT_NESTED_FOO")]
    foo: u32
}

const DEFAULT_NESTED_FOO: u32 = 2_u32.pow(19);
```

**Notes:**

- I don't really like `param` attribute name. Maybe `partial` instead? Or, if `Partial` is renamed to `Layer`[^2], the attribute might also be `layer`.
- `default = "<...>"` simply inlines whatever is written in `<...>` into a field initialisation in `Partial::default`.
  - `default` inlines `Default::default()`
- `partial` tells the macro that `Nested` implements `trait HasPartial` (see below), and that each method (see below) should be delegated to that nested partial
  - `partial = "CustomPartial"` tells the macro the field's type in the generated partial should be `CustomPartial`

This macro will generate a partial for `struct Config` (and for `struct Nested`):

```rust
#[derive(Serialize, Deserialize)]
struct ConfigPartial {
    foo: Option<u32>,
    bar: Option<bool>,
    baz: Option<String>,
    log_level: Option<LogLevel>,
    optional_param: Option<u64>,
    nested: <Nested as HasPartial>::Partial
}

impl Partial for ConfigPartial {
    type Resolved = Config;
    
    // ...
}

impl HasPartial for Config {
    type Partial = ConfigPartial;
}
```

**Notes:**

- `nested` field **is not** wrapped into `Option<..>`
- Hmm, now each field is wrapped into `Option<T>`. However, I can create a `struct DefaultPartial<T>` (or `DummyPartial<T>`, or `SimplePartial<T>`), so the macro will be able to work with ALL fields as nested partials. It simplifies macro, but makes generated code more verbose and there _might be_ a possibility that the produced code will have some runtime overhead, but I think the compiler might optimise it away. TODO: make research in godbolt.
- `HasPartial` marker trait helps user to write less code

Here we came to the `trait Partial`:

```rust
pub trait Partial {
    type Resolved;

    /// Construct a partial with all empty fields.
    fn new() -> Self;

    /// Construct a partial with default values.
    fn default() -> Self;
    
    /// Construct a partial from environment variables.
    fn from_env(provider: &impl EnvProvider) -> Result<Self, Report>
    where
        Self: Sized;

    /// Merge a partial with another partial.
    ///
    /// Merge strategies might be customised through macro attributes. (TODO)
    fn merge(self, other: Self) -> Self;

    /// "Unwrap" a partial with all required fields presented as actual values instead
    /// of [`Option`]s.
    fn resolve(self) -> Result<Self::Resolved, MissingFieldsError>;
}
```

**Notes:**

- Having `Partial::default` instead of `Default::default` is needed for nesting partials into each other. Although, `trait Partial` might be a super trait of `trait Default`, I am not sure in general that `trait Default` semantics are applicable to `Partial::default`. I would call the latter as `Partial::with_defaults`, while `Default::default` is more like `Partial::new()`, which produces an empty partial.
- My inner perfectionist tells me to move `from_env`[^1] into a separate trait, because it is "not generic enough". It forces `trait Partial`'s binding with ENV semantics, which "might not be always the case". Can `from_env` implementation be toggled with `#[param(from_env)]` attribute? Or... make a default implementation of `from_env` which simply returns `Ok(Self::new())`? Furthermore, toggle `from_env` with a crate-level feature flag, like Clap does it?
  - If it becomes `from_key_value`, it makes even more sense to make it an optional feature of the macro and not a part of `trait Partial`. Needs research.
- `from_env` and `resolve` emit batch errors for all fields (and nested partials) at once. (wip) 
- Rename `::resolve` to `::complete`? I don't like `::build` or `::unwrap`, as they don't translate the right thing.

The macro does not have much logic by itself. It generates a boilerplate which is based on the main library

## How to use partials

This section is raw. Currently, you can use bare partials:

```rust
// #[derive(Partial)]
// struct Sample { .. }

const INPUT: &str = r#"
required_baz = false
"#;

let sample = <Sample as HasPartial>::Partial::default()
    .merge(toml::from_str(INPUT).unwrap())
    .merge(<Sample as HasPartial>::Partial::from_env(
        &soukousei::env::StdEnv::new(),
    )?)
    .resolve()
    .into_diagnostic()?;
```

It is inconvenient and requires extra work if you need better errors (primarily related to deserialization).

I am thinking about "Source API", which will take responsibility of:

- Support format-related (JSON/TOML/etc) ser/de
- Switch formats support with feature flags
- Enhance error reporting with format-specific details, spans, paths, metadata like file name, etc 
- Provide common inter-source scenarios like reading from a file, or construction from a string, or loading from url, or whatever
- Composing multiple sources into a "higher-level" source which can e.g. sequentially try different sources under the hood. Use case: supporting different configuration formats / file names.

I imagine it like this:

```rust
use soukousei::{Source, source::{Toml, Json, TryMultiple}};

let sample: Sample = Source::<Sample>::new()
    .merge_defaults()
    .merge_source(
        TryMultiple::new(vec![
            Toml::file("config.toml"),
            Json::file("config.json")
        ])
    )?
    .merge_env()?
    .resolve()?;
```

Reference: [`figment`'s Provider API](https://docs.rs/figment/latest/figment/trait.Provider.html).

## Custom partials

Nesting partials into each other allows not only have nested configurations, but also to write your own `impl Partial` for a whatever type you want. This way you can have full control on:

- Field's `new` and `default` values
- Field-level merge strategy
- Field-level env loading and parsing[^1]
- Even field-level resolution

## Notes and questions

- In order to customise how generated partials are ser/de, you might pass `#[param(serde(...))]` attributes, so that `Partial` macro can simply propagate them
- **Testability:** great. It should be easy to cover the whole partials resolution process (defaults, deserialisation, env loading, merging and final unwrap) with unit tests.
- Abstract environment loading away by making it just one of existing sources (like in figment)? This way, the macro will not have any `env`-related fields. Instead, there should be a good mechanism of mapping ENV variables to partial fields. It requires further research.
- `derive` feature flag for `soukousei_derive` re-export (e.g. `serde`, `clap`)? TODO: read about idiomatic usage of feature flags in general.
- Enable `from_env` with a feature flag (e.g. `clap`)? It is easy to switch off with `#[cfg]`.
- What visibility should the generated partials and their fields have? Should it be simply inherited from the parent struct?
- Should `Partial::merge` support errors? It doesn't seem to be a common case, but supporting it is easy and _ideally_ doesn't have a runtime cost for trivial strategies.
- Advanced merge strategies, when it might depend on the state of other fields? Overkill and out of scope of soukousei, IMO.

## TODO

- [ ] Enhance "batch error" mechanism. Make it generic enough to support both `resolve` and `from_env` functions 
- [ ] Consistent naming of structs, traits, and attributes
- [ ] A field-level `parse_env`[^1] attribute to override how fetched ENV var is parsed (default is `FromStr::from_str`). **Could be manually implemented with a custom `Partial` ([see](#custom-partials)).**
- [ ] A field-level `merge` attribute to customise the default "replace with newer" strategy. **Could be manually implemented with a custom `Partial` ([see](#custom-partials)).**
- [ ] Establish "Sources API", probably similar to [`figment`'s Provider API](https://docs.rs/figment/latest/figment/trait.Provider.html)
- [ ] Research with the [Compiler Explorer](https://godbolt.org/) how does wrapping ALL fields into partials affects compiled code. Although, this library is not about performance, but about UX. Although, using bare partials might be performant enough, but is it useful?

## Acknowledgments

- [`schematic`](https://docs.rs/schematic), for partials design
- [`figment`](https://docs.rs/figment), for sources design 

[^1]: I am thinking more and more about moving `from_env` out of `trait Partial`. Generalise it to `from_key_value`?

[^2]: Most probably will rename to `Layer`