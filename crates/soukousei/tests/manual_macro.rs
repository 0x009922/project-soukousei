extern crate core;

mod util;

use miette::{miette, IntoDiagnostic, Report, WrapErr};
use serde::{Deserialize, Serialize};
use soukousei::{env::EnvProvider, HasPartial, MissingFieldsError, Partial};
use std::collections::HashMap;
use std::str::FromStr;
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

    fn from_env(_provider: &impl EnvProvider) -> Result<Self, Report>
    where
        Self: Sized,
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

    fn resolve(self) -> Result<Self::Resolved, MissingFieldsError> {
        self.0.resolve()
    }
}

// MACRO OUTPUT

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

    fn from_env(provider: &impl EnvProvider) -> Result<Self, Report>
    where
        Self: Sized,
    {
        // TODO: collect errors in a batch?

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

    fn resolve(self) -> Result<Self::Resolved, MissingFieldsError> {
        let mut missing_fields = MissingFieldsError::dummy();

        if self.with_default_foo.is_none() {
            missing_fields = missing_fields.add_field("with_default_foo")
        }

        if self.required_baz.is_none() {
            missing_fields = missing_fields.add_field("required_baz")
        }

        let nested = match self.nested.resolve() {
            Ok(value) => Some(value),
            Err(err) => {
                missing_fields = missing_fields.nest("nested", err);
                None
            }
        };

        let custom = match self.custom.resolve() {
            Ok(value) => Some(value),
            Err(err) => {
                missing_fields = missing_fields.nest("custom", err);
                None
            }
        };

        missing_fields.result()?;

        Ok(Self::Resolved {
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

// #[derive(Config)]
#[derive(Debug)]
struct Nested {
    // #[param(env = "FOO", default = r#""I am default foo!".to_owned()"#)]
    foo_env: String,
    // #[param(env = ["SPECIFIC_BAR", "BAR"])]
    bar_env_multiple: Option<u32>,
}

// MACRO OUTPUT

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

    fn from_env(provider: &impl EnvProvider) -> Result<Self, Report>
    where
        Self: Sized,
    {
        Ok(Self {
            // TODO: specify path to variables as well
            foo_env: provider.fetch("FOO")?,
            bar_env_multiple: {
                provider
                    .fetch_from_arr(["SPECIFIC_BAR", "BAR"])?
                    .map(|(str_value, var_name)| {
                        // TODO: add a way to use something different than `from_str`
                        u32::from_str(&str_value)
                            .into_diagnostic()
                            .wrap_err_with(|| {
                                miette!(
                                    "Cannot parse `{}` env variable into a value from str",
                                    var_name
                                )
                            })
                    })
                    .transpose()?
            },
        })
    }

    fn merge(self, other: Self) -> Self {
        Self {
            foo_env: other.foo_env.or(self.foo_env),
            bar_env_multiple: other.bar_env_multiple.or(self.bar_env_multiple),
        }
    }

    fn resolve(self) -> Result<Self::Resolved, MissingFieldsError> {
        let mut missing_fields = MissingFieldsError::dummy();

        if self.foo_env.is_none() {
            missing_fields = missing_fields.add_field("foo_env");
        }

        missing_fields.result()?;

        Ok(Self::Resolved {
            foo_env: self.foo_env.unwrap(),
            bar_env_multiple: self.bar_env_multiple,
        })
    }
}

// MACRO OUTPUT END

#[test]
fn success_build_from_toml() -> Result<(), Report> {
    const INPUT: &str = r#"
    required_baz = false
    "#;

    let sample = <Sample as HasPartial>::Partial::new()
        .merge(toml::from_str(INPUT).unwrap())
        .merge(<Sample as HasPartial>::Partial::from_env(
            soukousei::env::StdEnv::new() & TestEnv::new().add("FOO", "SELECT foo FROM env"),
        )?)
        .resolve()
        .into_diagnostic()?;

    dbg!(&sample);

    Ok(())
}
