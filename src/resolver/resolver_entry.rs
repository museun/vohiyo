use std::{borrow::Borrow, hash::Hash};

use hashbrown::HashMap;

use super::Ready;

pub struct ResolverEntry<'a, K, V> {
    pub(in crate::resolver) inner: &'a mut HashMap<K, Ready<V>>,
}

impl<'a, K, V> ResolverEntry<'a, K, V> {
    pub fn set(&mut self, key: K, value: V)
    where
        K: Hash + PartialEq + Eq,
    {
        use hashbrown::hash_map::Entry::*;
        let value = Ready::Ready(value);
        match self.inner.entry(key) {
            Occupied(mut entry) => {
                *entry.get_mut() = value;
            }
            Vacant(entry) => {
                entry.insert(value);
            }
        }
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Hash + Eq,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        self.inner.get_mut(key).and_then(Ready::as_option_mut)
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Hash + Eq,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        self.inner.remove(key).and_then(Ready::into_option)
    }
}
