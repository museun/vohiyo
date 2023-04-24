use std::{borrow::Borrow, hash::Hash};

use hashbrown::HashMap;

use super::{Fut, Ready, ResolverEntry};

pub struct ResolverMap<K, V, T> {
    map: HashMap<K, Ready<V>>,
    pending: Vec<Fut<T>>,
}

impl<K, V, T> ResolverMap<K, V, T>
where
    K: Hash + PartialEq + Eq,
    T: Send + 'static,
{
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        self.map.contains_key(key)
    }

    pub fn try_get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        self.map.get(key).and_then(Ready::as_option)
    }

    pub fn get_or_update<Q>(&mut self, key: &Q, mut update: impl FnMut(&Q) -> Fut<T>) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        use hashbrown::hash_map::RawEntryMut::*;
        match self.map.raw_entry_mut().from_key(key) {
            Occupied(entry) => entry.into_mut().as_option(),
            Vacant(entry) => {
                entry.insert(key.to_owned(), Ready::NotReady);
                self.pending.push(update(key));
                None
            }
        }
    }

    pub fn get_or_else<Q>(&mut self, key: &Q, mut or_else: impl FnMut(&Q)) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        use hashbrown::hash_map::RawEntryMut::*;
        match self.map.raw_entry_mut().from_key(key) {
            Occupied(entry) => entry.into_mut().as_option(),
            Vacant(entry) => {
                entry.insert(key.to_owned(), Ready::NotReady);
                or_else(key);
                None
            }
        }
    }

    pub fn add(&mut self, fut: Fut<T>) {
        self.pending.push(fut)
    }

    pub fn update(&mut self) -> ResolverEntry<'_, K, V> {
        ResolverEntry {
            inner: &mut self.map,
        }
    }

    pub fn remove_by_key<Q>(&mut self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        self.map.remove(key).is_some()
    }

    pub fn ready_iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map
            .iter()
            .filter_map(|(k, v)| v.as_option().map(|v| (k, v)))
    }

    pub fn retain(&mut self, func: impl FnMut(&K, &mut Ready<V>) -> bool) {
        self.map.retain(func)
    }

    pub fn is_ready<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + PartialEq + Eq + ToOwned<Owned = K> + ?Sized,
    {
        self.map.get(key).filter(|r| r.is_ready()).is_some()
    }

    pub fn poll(&mut self, mut resolve: impl FnMut(&mut ResolverEntry<'_, K, V>, T)) {
        self.pending.retain_mut(|item| {
            let Some(item) = item.try_resolve() else { return true };
            let mut entry = ResolverEntry {
                inner: &mut self.map,
            };

            resolve(&mut entry, item);
            false
        })
    }
}
