use std::cell::UnsafeCell;
use std::collections::{HashMap, hash_map};
use std::hash::{Hash, Hasher};
use std::mem::{self, ManuallyDrop};

// Create a wrapper around HashMap that allows for using the Entry API with
// reference types: this prevents us from having to clone the data if there's
// already an entry for the station

struct RefKey<K>(UnsafeCell<ManuallyDrop<K>>)
    where
        K: Clone + Hash + Eq;

impl<K> Eq for RefKey<K> where K: Clone + Hash + Eq {}

impl<K> PartialEq for RefKey<K>
    where
        K: Clone + Hash + Eq,
{
    fn eq(&self, other: &Self) -> bool {
        unsafe { (*self.0.get()).eq(&*other.0.get()) }
    }
}

impl<K> Hash for RefKey<K>
    where
        K: Clone + Hash + Eq,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe { (*self.0.get()).hash(state) }
    }
}

impl<K> RefKey<K>
    where
        K: Clone + Hash + Eq,
{
    fn new(val: K) -> Self {
        Self(UnsafeCell::new(ManuallyDrop::new(val)))
    }

    fn inner(self) -> K {
        ManuallyDrop::into_inner(self.0.into_inner())
    }

    // this leaks if it was already allocated, but we don't ever call it twice
    fn alloc(&self) {
        let ptr = self.0.get();
        unsafe {
            let alloc = (*ptr).clone();
            ptr.write(alloc);
        }
    }
}

pub struct RefHashMap<K, V>(HashMap<RefKey<K>, V>)
    where
        K: Clone + Hash + Eq;

impl<K, V> RefHashMap<K, V>
    where
        K: Clone + Hash + Eq,
{
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity(capacity))
    }

    pub(crate) fn entry_ref<'rf, 'map, R: 'rf, T>(
        &'map mut self,
        reference: R,
    ) -> Entry<'rf, K, V>
        where
        // If the reference can act like a slice<T> and K can make itself from Vec<T>, then we can fake it:
        // pretend the slice is actually a vector, then turn that vector into K
        // This is O.K. so long as we never:
        //  - mutate the fake K (HashMap itself doesn't, and we allocate the K if it ends up in the HashMap,
        //      so the HashMap never holds reference types )
        //  - drop the fake K (we wrap it in a ManuallyDrop when we pass it to RefKey)
            'map: 'rf,
            R: AsRef<[T]>,
            K: From<Vec<T>>,
    {
        let slice = reference.as_ref();
        let vec: Vec<T> = unsafe {
            Vec::from_raw_parts(mem::transmute(slice.as_ptr()), slice.len(), slice.len())
        };
        let key: K = vec.into();
        Entry(self.0.entry(RefKey::new(key)))
    }
}

pub struct IntoIter<K, V>(hash_map::IntoIter<RefKey<K>, V>)
    where
        K: Clone + Hash + Eq;

impl<K, V> Iterator for IntoIter<K, V>
    where
        K: Clone + Hash + Eq,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, v)| (k.inner(), v))
    }
}

impl<K, V> IntoIterator for RefHashMap<K, V>
    where
        K: Clone + Hash + Eq,
{
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.0.into_iter())
    }
}

pub struct Entry<'a, K, V>(hash_map::Entry<'a, RefKey<K>, V>)
    where
        K: Clone + Hash + Eq;

impl<'a, K, V> Entry<'a, K, V>
    where
        K: Clone + Hash + Eq,
{
    pub(crate) fn and_modify<F>(self, f: F) -> Self
        where
            F: FnOnce(&mut V),
    {
        let entry = self.0.and_modify(f);
        Self(entry)
    }

    pub(crate) fn or_insert_with<F>(self, default: F) -> &'a mut V
        where
            F: FnOnce() -> V,
    {
        // key part of the whole process: if we have to insert, we perform the allocation
        // this means that we won't ever have keys in the HashMap that are references
        self.0.or_insert_with_key(|key| {
            key.alloc();
            default()
        })
    }
}
