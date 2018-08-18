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
