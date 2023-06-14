extern crate core;

mod util;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use totonoeru::{env::EnvProvider, HasPartial, Partial, ResolveErrorResultExt};
use totonoeru::{Config, ResolveError};
use util::TestEnv;

#[derive(Debug)]
struct Sample {
    // #[param(default = "100")]
    with_default_foo: u32,
    optional_bar: Option<String>,
    required_baz: bool,
    // #[param(partial)]
    // same as
    // #[param(partial = "<Nested as HasPartial>::Partial")
    nested: Nested,
    // #[param(partial = "CustomPartial")]
    custom: u32,
    // TODO: use case with partial through a final type newtype HasType
}

// we only want to override how merge works
#[derive(Debug, Serialize, Deserialize)]
struct CustomPartial(Option<u32>);

impl Partial for CustomPartial {
    type Resolved = u32;

    fn new() -> Self {
        Self(None)
    }

    fn default() -> Self {
        Self(Some(100))
    }

    fn from_env<P, E>(_provider: &P) -> Result<Self, E>
    where
        Self: Sized,
        P: EnvProvider<FetchError = E>,
    {
        Ok(Self::new())
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

    fn resolve(self) -> Result<Self::Resolved, ResolveError> {
        self.0.resolve()
    }
}

// what macro should generate

impl HasPartial for Sample {
    type Partial = SamplePartial;
}

#[derive(Serialize, Deserialize, Debug)]
struct SamplePartial {
    with_default_foo: Option<u32>,
    optional_bar: Option<String>,
    required_baz: Option<bool>,
    #[serde(default = "Partial::new")]
    nested: <Nested as HasPartial>::Partial,
    #[serde(default = "Partial::new")]
    custom: CustomPartial,
}

impl Partial for SamplePartial {
    type Resolved = Sample;

    fn new() -> Self {
        Self {
            with_default_foo: None,
            optional_bar: None,
            required_baz: None,
            nested: Partial::new(),
            custom: Partial::new(),
        }
    }

    fn default() -> Self {
        Self {
            with_default_foo: Some(100),
            optional_bar: None,
            required_baz: None,
            nested: Partial::default(),
            custom: Partial::default(),
        }
    }

    fn from_env<P, E>(provider: &P) -> Result<Self, E>
    where
        Self: Sized,
        P: EnvProvider<FetchError = E>,
    {
        Ok(Self {
            with_default_foo: None,
            optional_bar: None,
            required_baz: None,
            nested: Partial::from_env(provider)?,
            custom: Partial::from_env(provider)?,
        })
    }

    fn merge(self, other: Self) -> Self {
        Self {
            with_default_foo: other.with_default_foo.or(self.with_default_foo),
            optional_bar: other.optional_bar.or(self.optional_bar),
            required_baz: other.required_baz.or(self.required_baz),
            nested: self.nested.merge(other.nested),
            custom: self.custom.merge(other.custom),
        }
    }

    fn resolve(self) -> Result<Self::Resolved, ResolveError> {
        Ok(Self::Resolved {
            with_default_foo: self
                .with_default_foo
                .ok_or(ResolveError::new().with_loc("with_default_foo"))?,
            optional_bar: self.optional_bar,
            required_baz: self
                .required_baz
                .ok_or(ResolveError::new().with_loc("required_baz"))?,
            nested: self.nested.resolve().with_loc("nested")?,
            custom: self.custom.resolve().with_loc("custom")?,
        })
    }
}

// #[derive(Config)]
#[derive(Debug)]
struct Nested {
    // #[param(env = "FOO", default = r#""I am default foo!".to_owned()"#)]
    foo_env: String,
    // #[param(env = ["SPECIFIC_BAR", "BAR"])]
    bar_env_multiple: Option<u32>,
}

impl HasPartial for Nested {
    type Partial = NestedPartial;
}

#[derive(Serialize, Deserialize, Debug)]
struct NestedPartial {
    foo_env: Option<String>,
    bar_env_multiple: Option<u32>,
}

impl Partial for NestedPartial {
    type Resolved = Nested;

    fn new() -> Self {
        Self {
            foo_env: None,
            bar_env_multiple: None,
        }
    }

    fn default() -> Self {
        Self {
            foo_env: Some("I am default foo!".to_owned()),
            bar_env_multiple: None,
        }
    }

    fn from_env<P, E>(provider: &P) -> Result<Self, E>
    where
        Self: Sized,
        P: EnvProvider<FetchError = E>,
    {
        Ok(Self {
            foo_env: provider.fetch("FOO")?,
            bar_env_multiple: {
                provider
                    .fetch_from_iter(["SPECIFIC_BAR", "BAR"].iter())?
                    .map(|x|
                    // FIXME: add a way to handle parsing errors as well
                    u32::from_str(&x).unwrap())
            },
        })
    }

    fn merge(self, other: Self) -> Self {
        Self {
            foo_env: other.foo_env.or(self.foo_env),
            bar_env_multiple: other.bar_env_multiple.or(self.bar_env_multiple),
        }
    }

    fn resolve(self) -> Result<Self::Resolved, ResolveError> {
        // TODO: collect all missing field in a bulk
        Ok(Self::Resolved {
            foo_env: self
                .foo_env
                .ok_or(ResolveError::new().with_loc("foo_env"))?,
            bar_env_multiple: self.bar_env_multiple,
        })
    }
}

#[test]
fn success_build_from_toml() {
    const input: &str = r#"
    required_baz = false
    "#;

    let partial = <Sample as HasPartial>::Partial::default()
        .merge(toml::from_str(input).unwrap())
        .merge(
            <Sample as HasPartial>::Partial::from_env(
                &TestEnv::new().add("FOO", "SELECT foo FROM env"),
            )
            .unwrap(),
        )
        .resolve()
        .unwrap();

    dbg!(&partial);
}
