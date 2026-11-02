use std::collections::HashMap;

#[derive(Default)]
pub struct Config {
    pub settings: HashMap<String, String>,
    pub custom_command: Vec<String>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }
}
