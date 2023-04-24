#![cfg_attr(debug_assertions, allow(dead_code, unused_variables,))]
use std::{borrow::Borrow, future::Future, hash::Hash};

use hashbrown::HashMap;
use tokio::sync::oneshot;

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

pub struct ResolverEntry<'a, K, V> {
    inner: &'a mut HashMap<K, Ready<V>>,
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

pub struct Fut<T> {
    recv: oneshot::Receiver<T>,
}

impl<T> Fut<T>
where
    T: Send + 'static,
{
    pub const fn new(recv: oneshot::Receiver<T>) -> Self {
        Self { recv }
    }

    pub fn wrap<E>(self, wrap: impl FnOnce(T) -> E + Send + Sync + 'static) -> Fut<E>
    where
        E: Send + 'static,
    {
        <Fut<E>>::spawn(async { wrap(self.wait().await.expect("resolver future shouldn't panic")) })
    }

    pub fn spawn(fut: impl Future<Output = T> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let result = fut.await;
            let _ = tx.send(result);
        });
        Self { recv: rx }
    }

    pub fn try_resolve(&mut self) -> Option<T> {
        self.recv.try_recv().ok()
    }

    pub async fn wait(self) -> Option<T> {
        self.recv.await.ok()
    }
}

pub enum Ready<V> {
    Ready(V),
    NotReady,
}

impl<V> Ready<V> {
    const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    const fn as_option(&self) -> Option<&V> {
        match self {
            Self::Ready(val) => Some(val),
            Self::NotReady => None,
        }
    }

    fn into_option(self) -> Option<V> {
        match self {
            Self::Ready(val) => Some(val),
            Self::NotReady => None,
        }
    }

    fn as_option_mut(&mut self) -> Option<&mut V> {
        match self {
            Self::Ready(val) => Some(val),
            Self::NotReady => None,
        }
    }
}
