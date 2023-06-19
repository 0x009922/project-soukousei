mod util;

use soukousei::env::EnvProvider;
use soukousei::Config;
use std::collections::HashMap;
use std::num::NonZeroU64;
use toml::toml;
use util::TestEnv;

#[derive(Config)]
struct Test {
    #[param(default = "100")]
    with_default_foo: u32,
    optional_bar: Option<String>,
    required_baz: bool,
    #[param(nested)]
    nested: Nested,
}

#[derive(Config)]
struct Nested {
    #[param(env = "FOO", default = r#""I am default foo!".to_owned()"#)]
    foo_env: String,
    #[param(envs = ["SPECIFIC_BAR", "BAR"])]
    bar_env_multiple: Option<u32>,
}

#[test]
fn success_build_from_toml() {
    const input: &str = r#"
    required_baz = false
    "#;

    let config: Test = Test::Partial::default()
        .parse_and_merge(toml::de::ValueDeserializer::new(input))?
        .parse_env_and_merge(TestEnv::new().add("FOO", "SELECT foo FROM env"))?
        .resolve()?;
}
