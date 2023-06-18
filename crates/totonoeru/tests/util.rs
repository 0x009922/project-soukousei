use std::collections::HashMap;
use totonoeru::{env::EnvProvider, miette::Report};

pub struct TestEnv {
    map: HashMap<String, String>,
}

impl TestEnv {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn add(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.map
            .insert(key.as_ref().to_owned(), value.as_ref().to_owned());
        self
    }
}

impl EnvProvider for TestEnv {
    fn fetch(&self, key: impl AsRef<str>) -> Result<Option<String>, Report> {
        Ok(self.map.get(key.as_ref()).cloned())
    }
}
