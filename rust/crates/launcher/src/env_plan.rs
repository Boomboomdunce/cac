#[derive(Clone, Debug, Default)]
pub struct EnvPlan {
    entries: Vec<(String, String)>,
    removals: Vec<String>,
}

impl EnvPlan {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            removals: Vec::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        self.removals.retain(|candidate| candidate != &key);
        self.entries.retain(|(candidate, _)| candidate != &key);
        self.entries.push((key, value.into()));
    }

    pub fn unset(&mut self, key: impl Into<String>) {
        let key = key.into();
        self.entries.retain(|(candidate, _)| candidate != &key);
        self.removals.retain(|candidate| candidate != &key);
        self.removals.push(key);
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.entries.iter()
    }

    pub fn latest_value(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn removals(&self) -> impl Iterator<Item = &String> {
        self.removals.iter()
    }
}
