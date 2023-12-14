pub mod search;
pub use search::{SearchOperator, SortOrder};

pub mod timestamp;
pub use timestamp::{NanosUtc, SecondsUtc};

use std::collections::HashMap;
use std::hash::Hash;

pub trait HashVec {
    type Elem;

    fn to_map_keep_all<F, K, V>(self, f: F) -> HashMap<K, Vec<V>>
    where
        F: Fn(Self::Elem) -> (K, V),
        K: Eq + Hash;

    fn to_map_keep_last<F, K, V>(self, f: F) -> HashMap<K, V>
    where
        F: Fn(Self::Elem) -> (K, V),
        K: Eq + Hash;
}

impl<T, Elem> HashVec for T
where
    T: IntoIterator<Item = Elem>,
{
    type Elem = Elem;

    fn to_map_keep_all<F, K, V>(self, f: F) -> HashMap<K, Vec<V>>
    where
        F: FnMut(Elem) -> (K, V),
        K: Eq + Hash,
    {
        let iter = self.into_iter();
        let mut map: HashMap<_, Vec<_>> = HashMap::with_capacity(iter.size_hint().0);
        for (k, v) in iter.map(f) {
            map.entry(k).or_default().push(v);
        }
        map
    }

    fn to_map_keep_last<F, K, V>(self, f: F) -> HashMap<K, V>
    where
        F: FnMut(Self::Elem) -> (K, V),
        K: Eq + Hash,
    {
        self.into_iter().map(f).collect()
    }
}
