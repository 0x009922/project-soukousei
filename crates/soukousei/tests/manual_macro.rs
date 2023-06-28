extern crate core;

mod util;

use miette::{miette, IntoDiagnostic, Report, WrapErr};
use serde::{Deserialize, Serialize};
use soukousei::env::{FieldFromEnvError, FromEnv};
use soukousei::{
    env::EnvProvider, CompleteError, HasLayer, Layer, MissingFieldError, MultipleFieldsError,
    ResultExt,
};
use std::collections::HashMap;
use std::str::FromStr;
use util::TestEnv;

#[derive(Debug)]
// #[derive(Layer)]
// #[layer(default, env)]
struct Sample {
    // #[layer(default = "100")]
    with_default_foo: u32,
    optional_bar: Option<String>,
    required_baz: bool,
    // #[layer(nested)]
    // same as
    // #[layer(nested = "<Nested as HasLayer>::Layer")
    nested: Nested,
    // #[layer(nested = "CustomPartial")]
    custom: u32,
    // TODO: use case with partial through a final type newtype HasType
}

// we only want to override how merge works
#[derive(Debug, Serialize, Deserialize)]
struct CustomLayer(Option<u32>);

impl Default for CustomLayer {
    fn default() -> Self {
        Self(Some(100))
    }
}

impl FromEnv for CustomLayer {
    fn from_env(
        _provider: &impl EnvProvider,
    ) -> Result<Self, MultipleFieldsError<FieldFromEnvError>>
    where
        Self: Sized,
    {
        Ok(Self::new())
    }
}

impl Layer for CustomLayer {
    type Complete = u32;

    fn new() -> Self {
        Self(None)
    }

    fn merge(self, other: Self) -> Self {
        let inner = match (self.0, other.0) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };

        Self(inner)
    }

    fn complete(self) -> Result<Self::Complete, CompleteError> {
        self.0.ok_or(CompleteError::MissingSelf)
    }
}

// MACRO OUTPUT

impl HasLayer for Sample {
    type Layer = SampleLayer;
}

#[derive(Serialize, Deserialize, Debug)]
struct SampleLayer {
    with_default_foo: Option<u32>,
    optional_bar: Option<String>,
    required_baz: Option<bool>,
    #[serde(default = "Layer::new")]
    nested: <Nested as HasLayer>::Layer,
    #[serde(default = "Layer::new")]
    custom: CustomLayer,
}

impl Default for SampleLayer {
    fn default() -> Self {
        Self {
            with_default_foo: Some(100),
            optional_bar: None,
            required_baz: None,
            nested: Default::default(),
            custom: Default::default(),
        }
    }
}

impl FromEnv for SampleLayer {
    fn from_env(provider: &impl EnvProvider) -> Result<Self, MultipleFieldsError<FieldFromEnvError>>
    where
        Self: Sized,
    {
        let errors = MultipleFieldsError::new();

        let (nested, errors) = errors.nest_if_err(FromEnv::from_env(provider), "nested");

        let (custom, errors) = errors.nest_if_err(FromEnv::from_env(provider), "custom");

        errors.result()?;

        Ok(Self {
            with_default_foo: None,
            optional_bar: None,
            required_baz: None,
            nested: nested.unwrap(),
            custom: custom.unwrap(),
        })
    }
}

impl Layer for SampleLayer {
    type Complete = Sample;

    fn new() -> Self {
        Self {
            with_default_foo: None,
            optional_bar: None,
            required_baz: None,
            nested: Layer::new(),
            custom: Layer::new(),
        }
    }

    fn merge(self, other: Self) -> Self {
        Self {
            with_default_foo: other.with_default_foo.or(self.with_default_foo),
            optional_bar: other.optional_bar.or(self.optional_bar),
            required_baz: other.required_baz.or(self.required_baz),
            nested: Layer::merge(self.nested, other.nested),
            custom: Layer::merge(self.custom, other.custom),
        }
    }

    fn complete(self) -> Result<Self::Complete, CompleteError> {
        let errors = MultipleFieldsError::new();

        let errors = errors.add_if_none(&self.with_default_foo, "with_default_foo");

        let errors = errors.add_if_none(&self.required_baz, "required_baz");

        use soukousei::ResultExt;

        let (nested, errors) = self.nested.complete().nest_if_err(errors, "nested");

        let (custom, errors) = self.custom.complete().nest_if_err(errors, "custom");

        errors.result()?;

        Ok(Self::Complete {
            // TODO: use `expect` in macro code
            with_default_foo: self.with_default_foo.unwrap(),
            optional_bar: self.optional_bar,
            required_baz: self.required_baz.unwrap(),
            nested: nested.unwrap(),
            custom: custom.unwrap(),
        })
    }
}

// MACRO OUTPUT END

// #[derive(Layer)]
// #[layer(default, env)]
#[derive(Debug)]
struct Nested {
    // #[param(env = "FOO", default = r#""I am default foo!".to_owned()"#)]
    foo_env: String,
    // #[param(env = ["SPECIFIC_BAR", "BAR"])]
    bar_env_multiple: Option<u32>,
}

// MACRO OUTPUT

impl HasLayer for Nested {
    type Layer = NestedLayer;
}

#[derive(Serialize, Deserialize, Debug)]
struct NestedLayer {
    foo_env: Option<String>,
    bar_env_multiple: Option<u32>,
}

impl Default for NestedLayer {
    fn default() -> Self {
        Self {
            foo_env: Some("I am default foo!".to_owned()),
            bar_env_multiple: None,
        }
    }
}

impl FromEnv for NestedLayer {
    fn from_env(provider: &impl EnvProvider) -> Result<Self, MultipleFieldsError<FieldFromEnvError>>
    where
        Self: Sized,
    {
        let errors = MultipleFieldsError::new();

        let (foo_env, errors) = errors.add_if_err(
            "foo_env",
            provider.fetch_and_parse("FOO", soukousei::env::default_env_parse),
        );

        const BAR_ENV_MULTIPLE_VARIABLES: [&'_ str; 2] = ["SPECIFIC_BAR", "BAR"];

        let (bar_env_multiple, errors) = errors.add_if_err(
            "bar_env_multiple",
            provider.try_fetch_multiple_and_parse(
                BAR_ENV_MULTIPLE_VARIABLES.iter().map(|x| *x),
                soukousei::env::default_env_parse,
            ),
        );

        errors.result()?;

        Ok(Self {
            foo_env,
            bar_env_multiple,
        })
    }
}

impl Layer for NestedLayer {
    type Complete = Nested;

    fn new() -> Self {
        Self {
            foo_env: None,
            bar_env_multiple: None,
        }
    }

    fn merge(self, other: Self) -> Self {
        Self {
            foo_env: other.foo_env.or(self.foo_env),
            bar_env_multiple: other.bar_env_multiple.or(self.bar_env_multiple),
        }
    }

    fn complete(self) -> Result<Self::Complete, CompleteError> {
        let errors = MultipleFieldsError::new();

        let errors = errors.add_if_none(&self.foo_env, "foo_env");

        errors.result()?;

        Ok(Self::Complete {
            foo_env: self.foo_env.unwrap(),
            bar_env_multiple: self.bar_env_multiple,
        })
    }
}

// MACRO OUTPUT END

#[test]
fn success_build_from_toml() -> Result<(), Report> {
    const INPUT: &str = r#"
    # required_baz = false
    "#;

    let sample = <Sample as HasLayer>::Layer::default()
        .merge(toml::from_str(INPUT).unwrap())
        // .merge(<Sample as HasLayer>::Layer::from_env(
        //     soukousei::env::StdEnv::new() & TestEnv::new().add("FOO", "SELECT foo FROM env"),
        // )?)
        .complete()
        .map_err(|err| miette!("complete err: {err:?}"))?;
    // .map_err(|x| x.into_diagnostic())?;

    dbg!(&sample);

    Ok(())
}
