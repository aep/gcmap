//#![feature(test)]
//extern crate test;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct MarkOnDrop {
    marker: Arc<AtomicBool>,
    gc:     Arc<AtomicUsize>,
}

impl Drop for MarkOnDrop {
    fn drop(&mut self) {
        self.marker.store(true, Ordering::SeqCst);
        self.gc.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct HashMap<K, V> {
    v:  std::collections::HashMap<K, (V, Arc<AtomicBool>)>,
    gc: Arc<AtomicUsize>,
}

impl<K,V> Default for HashMap<K,V>
    where K: std::cmp::Eq + std::hash::Hash
{
    fn default() -> Self {
        HashMap {
            v:  std::collections::HashMap::new(),
            gc: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl<K, V> HashMap<K, V>
    where K: std::cmp::Eq + std::hash::Hash
{
    pub fn new() -> Self {
        Self::default()
    }
}


impl<K,V> HashMap<K,V>
    where K: std::cmp::Eq + std::hash::Hash
{
    pub fn insert(&mut self, k: K, v: V) -> (MarkOnDrop, Option<V>)
        where K: std::cmp::Eq + std::hash::Hash
    {
        self.gc();
        let mark = MarkOnDrop {
            marker: Arc::new(AtomicBool::new(false)),
            gc:     self.gc.clone(),
        };
        let old = match self.v.insert(k, (v, mark.marker.clone())) {
            None => None,
            Some((v, marker)) => {
                if marker.load(Ordering::SeqCst) == false {
                    Some(v)
                } else {
                    None
                }
            }
        };
        (mark, old)
    }

    pub fn get<Q: ?Sized>(&mut self, k: &Q) -> Option<&V>
        where Q: std::cmp::Eq + std::hash::Hash,
              K: std::borrow::Borrow<Q>,
    {
        let remove = if let Some((_, marker)) = self.v.get(k) {
            marker.load(Ordering::SeqCst)
        } else {
            false
        };

        if remove {
            self.v.remove(k);
        }

        self.v.get(k).map(|(v,_)|v)
    }

    pub fn get_mut<Q: ?Sized>(&mut self, k: &mut Q) -> Option<&mut V>
        where Q: std::cmp::Eq + std::hash::Hash,
              K: std::borrow::Borrow<Q>,
    {
        let remove = if let Some((_, marker)) = self.v.get(k) {
            marker.load(Ordering::SeqCst)
        } else {
            false
        };

        if remove {
            self.v.remove(k);
        }

        self.v.get_mut(k).map(|(v,_)|v)
    }

    pub fn len(&self) -> usize {
        self.v.len()
    }


    pub fn gc(&mut self) {
        if self.gc.load(Ordering::SeqCst) < self.len() / 2 {
            return;
        }
        self.gc.store(0, Ordering::SeqCst);
        //TODO to make gc more efficient, there should be multiple gc flags marking "regions"
        //but for that we need to modify the hashmap iterator
        self.v.retain(|_, (_, marker)| {
            !marker.load(Ordering::SeqCst)
        })
    }


    pub fn entry(&mut self, k: K) -> Entry<K, V> {
        self.gc();

        let remove = if let Some((_, marker)) = self.v.get(&k) {
            marker.load(Ordering::SeqCst)
        } else {
            false
        };

        if remove {
            self.v.remove(&k);
        }

        match self.v.entry(k) {
            std::collections::hash_map::Entry::Occupied(n) => {
                Entry::Occupied(OccupiedEntry{n})
            },
            std::collections::hash_map::Entry::Vacant(n) => {
                Entry::Vacant(VacantEntry{n, gc: self.gc.clone()})
            },
        }
    }
}


pub struct OccupiedEntry<'a, K: 'a, V: 'a>{
    n: std::collections::hash_map::OccupiedEntry<'a, K, (V,Arc<AtomicBool>)>,
}

pub struct VacantEntry<'a, K: 'a, V: 'a>{
    n: std::collections::hash_map::VacantEntry<'a, K, (V,Arc<AtomicBool>)>,
    gc: Arc<AtomicUsize>,
}

pub enum Entry<'a, K: 'a, V: 'a> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, K, V>),

    /// A vacant entry.
    Vacant(VacantEntry<'a, K, V>),
}


impl<'a, K, V> OccupiedEntry<'a, K, V> {
    pub fn into_mut(self) -> &'a mut V {
        &mut self.n.into_mut().0
    }
}

impl<'a, K, V> VacantEntry<'a, K, V> {
    pub fn insert_with<F: FnOnce(MarkOnDrop) -> V>(self, value: F) -> &'a mut V {
        let mark = MarkOnDrop {
            marker: Arc::new(AtomicBool::new(false)),
            gc:     self.gc.clone(),
        };
        let marker = mark.marker.clone();
        &mut (self.n.insert((value(mark), marker)).0)
    }
}

impl<'a, K, V> Entry<'a, K, V> {
    pub fn or_insert_with<F: FnOnce(MarkOnDrop) -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Occupied(entry)  => entry.into_mut(),
            Entry::Vacant(entry)    => {
                entry.insert_with(default)
            }
        }
    }
}





#[test]
fn entry() {
    let mut wm : HashMap<u32, u8> = HashMap::new();

    let mut holdme = None;

    {
        let val = wm.entry(1).or_insert_with(|mark|{
            holdme = Some(mark);
            2
        });
        *val = 3;
    }

    assert_eq!(wm.get(&1), Some(&3));
    drop(holdme);
    assert_eq!(wm.get(&1), None);

    {
        let val = wm.entry(1).or_insert_with(|mark|2);
        *val = 3;
    }

    assert_eq!(wm.get(&1), None);
}

#[test]
fn foo() {
    let mut wm : HashMap<u32, &'static str> = HashMap::new();
    let marks : Vec<MarkOnDrop> = (0..100000).map(|i|{
        let (mark, _) = wm.insert(i + 100000, "world");
        drop(mark);
        assert_eq!(wm.get(&(i + 100000)), None);
        let (mark, _) = wm.insert(i, "world");
        mark
    }).collect();
    assert_eq!(wm.get(&1), Some(&"world"));
    drop(marks);
    assert_eq!(wm.get(&1), None);
}


/*
#[cfg(test)]
mod tests {
    use super::*;
    use test::Bencher;

    #[bench]
    fn bla(b: &mut Bencher) {

        let mut wm : HashMap<u32, &'static str> = HashMap::new();

        b.iter(||{
            let marks : Vec<MarkOnDrop> = (0..100000).map(|i|{
                let (mark, _) = wm.insert(i + 100000, "world");
                drop(mark);
                assert_eq!(wm.get(&(i + 100000)), None);
                let (mark, _) = wm.insert(i, "world");
                mark
            }).collect();
            assert_eq!(wm.get(&1), Some(&"world"));
            drop(marks);
            assert_eq!(wm.get(&1), None);
        });
    }
}
*/
