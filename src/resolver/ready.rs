pub enum Ready<V> {
    Ready(V),
    NotReady,
}

impl<V> Ready<V> {
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub const fn as_option(&self) -> Option<&V> {
        match self {
            Self::Ready(val) => Some(val),
            Self::NotReady => None,
        }
    }

    pub fn into_option(self) -> Option<V> {
        match self {
            Self::Ready(val) => Some(val),
            Self::NotReady => None,
        }
    }

    pub fn as_option_mut(&mut self) -> Option<&mut V> {
        match self {
            Self::Ready(val) => Some(val),
            Self::NotReady => None,
        }
    }
}
