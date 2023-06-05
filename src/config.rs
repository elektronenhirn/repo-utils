use std::collections::HashMap;

pub struct Config {
    pub settings: HashMap<String, String>,
    pub custom_command: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            settings: HashMap::new(),
            custom_command: Vec::new(),
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }
}