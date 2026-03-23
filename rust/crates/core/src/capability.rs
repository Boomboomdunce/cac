use std::collections::BTreeSet;
use std::iter::FromIterator;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    inner: BTreeSet<String>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self {
            inner: BTreeSet::new(),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains(key)
    }

    pub fn insert(&mut self, key: impl Into<String>) -> bool {
        self.inner.insert(key.into())
    }

    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.inner.is_subset(&other.inner)
    }

    pub fn union(&self, other: &Self) -> Self {
        CapabilitySet {
            inner: self.inner.union(&other.inner).cloned().collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.inner.iter()
    }
}

impl<T> From<T> for CapabilitySet
where
    T: IntoIterator,
    T::Item: Into<String>,
{
    fn from(iter: T) -> Self {
        CapabilitySet {
            inner: BTreeSet::from_iter(iter.into_iter().map(Into::into)),
        }
    }
}
