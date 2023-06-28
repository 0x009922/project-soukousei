use crate::env::EnvProvider;
use miette::{Diagnostic, IntoDiagnostic, Report};
use serde::{Deserialize, Deserializer};
use std::fmt::{Display, Formatter, Write};
use std::ops::{Deref, Mul};
use thiserror::Error;

pub use miette;

pub mod env {
    use crate::{FieldsAcc, MultipleFieldsError};
    use miette::{miette, Report};
    use std::ffi::OsString;
    use std::ops::Deref;
    use std::str::FromStr;

    pub fn default_env_parse<T, E>(value: &str) -> Result<T, Report>
    where
        T: FromStr<Err = E>,
        E: std::error::Error,
    {
        FromStr::from_str(value)
            .map_err(|err| miette!("Failed to parse value from string: {}", err))
    }

    pub struct FieldFromEnvError {
        variable: String,
        report: Report,
    }

    impl FieldFromEnvError {
        pub fn new(report: Report, variable: String) -> Self {
            Self { report, variable }
        }
    }

    pub trait FromEnv {
        fn from_env(
            provider: &impl EnvProvider,
        ) -> Result<Self, MultipleFieldsError<FieldFromEnvError>>
        where
            Self: Sized;
    }

    impl MultipleFieldsError<FieldFromEnvError> {
        pub fn add_if_err<T>(
            self,
            loc: &'static str,
            result: Result<Option<T>, FieldFromEnvError>,
        ) -> (Option<T>, Self) {
            match result {
                Ok(value) => (value, self),
                Err(err) => (None, self.add(err, loc)),
            }
        }
    }

    pub trait EnvProvider {
        fn fetch(&self, key: impl AsRef<str>) -> Result<Option<String>, Report>;

        fn fetch_and_parse<T, F>(
            &self,
            key: &'static str,
            parse: F,
        ) -> Result<Option<T>, FieldFromEnvError>
        where
            F: FnOnce(&str) -> Result<T, Report>,
        {
            self.fetch(key)
                .map_err(|report| FieldFromEnvError::new(report, key.to_owned()))?
                .map(|raw| {
                    parse(&raw).map_err(|report| FieldFromEnvError::new(report, key.to_owned()))
                })
                .transpose()
        }

        fn try_fetch_multiple_and_parse<T, F>(
            &self,
            keys: impl Iterator<Item = &'static str>,
            parse: F,
        ) -> Result<Option<T>, FieldFromEnvError>
        where
            F: FnOnce(&str) -> Result<T, Report> + Copy,
        {
            // TODO: put all keys into errors?

            for key in keys {
                let value = self.fetch_and_parse(key, parse)?;
                if value.is_some() {
                    return Ok(value);
                }
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

#[derive(Error, Debug, Diagnostic)]
#[error("Missing field")]
pub struct MissingFieldError;

pub trait Layer {
    type Complete;

    fn new() -> Self;

    fn merge(self, other: Self) -> Self;

    fn complete(self) -> Result<Self::Complete, CompleteError>;
}

#[derive(Debug)]
pub enum CompleteError {
    MissingSelf,
    MissingFields(MultipleFieldsError<MissingFieldError>),
}

impl From<MultipleFieldsError<MissingFieldError>> for CompleteError {
    fn from(value: MultipleFieldsError<MissingFieldError>) -> Self {
        Self::MissingFields(value)
    }
}

pub trait ResultExt<T, E> {
    fn nest_if_err(
        self,
        errors: MultipleFieldsError<E>,
        loc: &'static str,
    ) -> (Option<T>, MultipleFieldsError<E>);
}

impl<T> ResultExt<T, MissingFieldError> for Result<T, CompleteError> {
    fn nest_if_err(
        self,
        mut errors: MultipleFieldsError<MissingFieldError>,
        loc: &'static str,
    ) -> (Option<T>, MultipleFieldsError<MissingFieldError>) {
        match self {
            Ok(value) => (Some(value), errors),
            Err(err) => {
                let errors = match err {
                    CompleteError::MissingFields(acc) => {
                        errors.fields.nest(acc.fields, loc);
                        errors
                    }
                    CompleteError::MissingSelf => errors.add(MissingFieldError, loc),
                };
                (None, errors)
            }
        }
    }
}

pub trait HasLayer {
    type Layer: Layer<Complete = Self>;
}

#[derive(Debug)]
pub struct FieldsAcc<T> {
    paths: Vec<WithPath<T>>,
}

impl<T> FieldsAcc<T> {
    pub fn new() -> Self {
        Self { paths: Vec::new() }
    }

    pub fn add_field(&mut self, value: T, loc: &'static str) {
        self.paths.push(WithPath::new(value).add_loc(loc));
    }

    pub fn nest(&mut self, other: Self, loc: &'static str) {
        for mut nested_path in other.paths.into_iter() {
            self.paths.push(nested_path.add_loc(loc));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.len() == 0
    }
}

#[derive(Debug)]
pub struct MultipleFieldsError<T> {
    fields: FieldsAcc<T>,
}

impl<T> MultipleFieldsError<T> {
    pub fn new() -> Self {
        Self {
            fields: FieldsAcc::new(),
        }
    }

    pub fn add(mut self, err: T, loc: &'static str) -> Self {
        self.fields.add_field(err, loc);
        self
    }

    pub fn nest(mut self, other: Self, loc: &'static str) -> Self {
        self.fields.nest(other.fields, loc);
        self
    }

    pub fn nest_if_err<U>(
        mut self,
        result: Result<U, Self>,
        loc: &'static str,
    ) -> (Option<U>, Self) {
        match result {
            Ok(value) => (Some(value), self),
            Err(err) => (None, self.nest(err, loc)),
        }
    }

    pub fn result(self) -> Result<(), Self> {
        if self.fields.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl<T> MultipleFieldsError<T>
where
    T: Diagnostic,
{
    pub fn into_diagnostic(self) -> FieldsErrorBunch<T> {
        let items = self
            .fields
            .paths
            .into_iter()
            .map(|WithPath { path, value }| FieldError {
                path: path.join("."),
                main: value,
            })
            .collect();

        FieldsErrorBunch { items }
    }
}

#[derive(Debug, Error, Diagnostic)]
#[error("FieldsErrorBunch")]
pub struct FieldsErrorBunch<T>
where
    T: Diagnostic,
{
    #[related]
    items: Vec<FieldError<T>>,
}

#[derive(Debug, Error, Diagnostic)]
#[error("{path}: {main}")]
pub struct FieldError<T>
where
    T: Diagnostic,
{
    path: String,
    #[diagnostic_source]
    main: T,
}

impl MultipleFieldsError<MissingFieldError> {
    pub fn add_if_none<T>(self, option: &Option<T>, loc: &'static str) -> Self {
        if option.is_none() {
            return self.add(MissingFieldError, loc);
        }
        self
    }
}

#[derive(Debug)]
pub struct WithPath<T> {
    path: Vec<&'static str>,
    value: T,
}

impl<T> WithPath<T> {
    pub fn new(value: T) -> Self {
        Self {
            path: Vec::new(),
            value,
        }
    }

    pub fn add_loc(mut self, loc: &'static str) -> Self {
        self.path.push(loc);
        self
    }
}
