use crate::env::EnvProvider;
use miette::{Diagnostic, IntoDiagnostic, Report};
use serde::{Deserialize, Deserializer};
use std::fmt::{Display, Formatter, Write};
use thiserror::Error;

pub use miette;

pub mod env {
    use miette::{miette, Report};
    use std::ffi::OsString;

    pub trait EnvProvider {
        fn fetch(&self, key: impl AsRef<str>) -> Result<Option<String>, Report>;

        fn fetch_from_arr<const N: usize>(
            &self,
            arr: [&'static str; N],
        ) -> Result<Option<(String, &'static str)>, Report> {
            for var_name in arr {
                let found = self.fetch(var_name)?;
                if let Some(found) = found {
                    return Ok(Some((found, var_name)));
                };
            }
            Ok(None)
        }
    }

    pub struct StdEnv;

    impl StdEnv {
        pub fn new() -> Self {
            Self
        }
    }

    impl EnvProvider for StdEnv {
        fn fetch(&self, key: impl AsRef<str>) -> Result<Option<String>, Report> {
            use std::env::{var, VarError};

            match var(key.as_ref()) {
                Ok(x) => Ok(Some(x)),
                Err(VarError::NotPresent) => Ok(None),
                Err(VarError::NotUnicode(os)) => {
                    // TODO: make a special diagnostic for this error
                    Err(miette!(
                        "ENV var `{}` is not a valid utf-8 string: {:?}",
                        key.as_ref(),
                        os
                    ))
                }
            }
        }
    }
}

pub mod sources {
    pub struct Json;

    trait FromFile {}
}

pub trait Layer {
    type Complete;

    fn new() -> Self;

    // fn from_env(provider: &impl EnvProvider) -> Result<Self, Report>
    // where
    //     Self: Sized;

    fn merge(self, other: Self) -> Self;

    fn complete(self) -> Result<Self::Complete, MissingFieldsError>;
}

pub trait HasLayer {
    type Layer: Layer<Complete = Self>;
}

#[derive(Error, Debug)]
pub struct MissingFieldsError {
    paths: Vec<Vec<&'static str>>,
}

impl Display for MissingFieldsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Missing required fields: ")?;

        for (i, field_path_reversed) in self.paths.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            f.write_str("`")?;
            for (i, loc) in field_path_reversed.iter().rev().enumerate() {
                if i > 0 {
                    f.write_str(".")?;
                }
                f.write_str(loc)?;
            }
            f.write_str("`")?;
        }
        Ok(())
    }
}

impl<T> IntoDiagnostic<T, MissingFieldsError> for MissingFieldsError {
    fn into_diagnostic(self) -> Result<T, Report> {
        todo!()
    }
}

impl MissingFieldsError {
    /// Empty missing fields accumulator
    pub fn dummy() -> Self {
        Self { paths: Vec::new() }
    }

    pub fn add_field(&mut self, loc: &'static str) {
        self.paths.push(vec![loc]);
    }

    pub fn nest(&mut self, loc: &'static str, other: Self) {
        for mut nested_path in other.paths.into_iter() {
            nested_path.push(loc);
            self.paths.push(nested_path);
        }
    }

    /// `Ok(())` if empty, `Err(Self)` otherwise
    pub fn result(self) -> Result<(), Self> {
        if self.paths.len() > 0 {
            Err(self)
        } else {
            Ok(())
        }
    }
}
