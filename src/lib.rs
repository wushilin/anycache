use std::sync::{Arc, RwLock};
use std::hash::Hash;
use tokio::sync::RwLock as TokioRwLock;


/// Provides a reliable cache for HashMap<Key, Value> where value can be derived from key, but could be expensive to generate.
/// The cache is thread safe and can be used in multi-threaded environment.
/// The cache is not async, so it is not suitable for async environment.
/// For async cache, use CasheAsync instead.
/// 
/// ```
/// use anycache::Cache;
/// use anycache::CacheAsync;
/// 
/// fn my_gen(x:&String) -> String {
///   println!("Generating {}", x);
///   let mut y = x.clone();
///   y.push_str("@");
///   y
/// }
/// 
/// // For async cache, use CacheAsync
/// fn test_sync_cache() {
///   let c = Cache::new(my_gen);
/// 
///   for j in 0..2 {
///     for i in 0..10 {
///       let key = format!("key{}", i);
///       let v = c.get(&key);
///       println!("{}:{}: {}", j, i, *v);
///     }
///   }
/// }
/// 
/// // For sync cache, use Cache
/// async fn test_cache_async() {
///   // Cache is only generated once above. Similarly, for Async.
/// 
///   let c = CacheAsync::new(my_gen);
/// 
///   for j in 0..2 {
///     for i in 0..10 {
///       let key = format!("key{}", i);
///       let v = c.get(&key).await;
///       println!("{}:{}: {}", j, i, *v);
///     }
///   }
/// }
/// 
/// ```


/// Create a Cache with K, V type. Similar to Map. 
/// However, the value is generated from key on-demand using generate function
pub struct Cache<K,V> where 
    K: Hash + std::cmp::Eq + Clone,
{
    map: Arc<RwLock<std::collections::HashMap<K, Arc<V>>>>,
    generator: Generator<K, V>,
}

// Same as Cache, but using Async RwLock
pub struct CacheAsync<K, V> where 
    K: Hash + std::cmp::Eq + Clone,
{
    map: Arc<TokioRwLock<std::collections::HashMap<K, Arc<V>>>>,
    generator: GeneratorAsync<K, V>,
}

/// A generator function place holder
struct Generator<K, V> where 
K:Hash + Eq + Clone 
{
    generator: Box<dyn Fn(&K) -> V + 'static + Sync>
}

struct GeneratorAsync<K, V> where 
    K:Hash + Eq + Clone 
{
    generator: Box<dyn Fn(&K) -> V + 'static + Sync>
}

impl <K, V> Cache<K,V> where 
    K:Hash + Eq + Clone 
{
    /// Create a new cache using the given generator function
    pub fn new(generator:impl Fn(&K) -> V + 'static + Sync) -> Self 
        where K: Hash + Eq {
        Cache {
            map: Arc::new(RwLock::new(std::collections::HashMap::new())),
            generator: Generator{generator: Box::new(generator)},
        }
    }

    /// Get from the cache. If the key is missing, do not generate it and return None
    pub fn get_if(&self, key: &K) -> Option<Arc<V>> {
        let r = self.map.read().unwrap();
        let value = r.get(key);
        match value {
            Some(v) => Some(Arc::clone(v)),
            None => None
        }
    }

    /// Drop the key from the cache if it exists
    /// Does nothing if the key is not there
    pub fn drop(&self, key: &K) {
        let mut w = self.map.write().unwrap();
        w.remove(key);
    }

    /// Get the key from cache. If not found, generate one.
    pub fn get(&self, key: &K) -> Arc<V> {
        let r = self.map.read().unwrap();
        let value = r.get(key);
        match value {
            Some(v) => Arc::clone(v),
            None => {
                drop(r);
                let mut w = self.map.write().unwrap();
                let value = (self.generator.generator)(key);
                let arc = Arc::new(value);
                w.insert(key.clone(), Arc::clone(&arc));
                drop(w);
                arc
            }
        }
    }
}


/// Similar to Cache, but using async RwLock
impl <K, V> CacheAsync<K,V> where 
    K:Hash + Eq + Clone 
{
    /// Create a new cache using the given generator function
    pub fn new(generator:impl Fn(&K) -> V + 'static + Sync) -> Self 
        where K: Hash + Eq {
        CacheAsync {
            map: Arc::new(TokioRwLock::new(std::collections::HashMap::new())),
            generator: GeneratorAsync{generator: Box::new(generator)},
        }
    }

    /// Get from the cache. If the key is missing, do not generate it and return None
    pub async fn get_if(&self, key: &K) -> Option<Arc<V>> {
        let r = self.map.read().await;
        let value = r.get(key);
        match value {
            Some(v) => Some(Arc::clone(v)),
            None => None
        }
    }

    /// Drop the key from the cache if it it exists
    pub async fn drop(&self, key: &K) {
        let mut w = self.map.write().await;
        w.remove(key);
    }
    /// Get the key from cache. If not found, generate one.
    pub async fn get(&self, key: &K) -> Arc<V> {
        let r = self.map.read().await;
        let value = r.get(key);
        match value {
            Some(v) => Arc::clone(v),
            None => {
                drop(r);
                let mut w = self.map.write().await;
                let value = (self.generator.generator)(key);
                let arc = Arc::new(value);
                w.insert(key.clone(), Arc::clone(&arc));
                drop(w);
                arc
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        println!("Running test...");
        let c = Cache::new(|x:&String| -> String {
            println!("Generating {}", x);
            let mut y = x.clone();
            y.push_str("@");
            y
        });

        for j in 0..2 {
            for i in 0..10 {
                let key = format!("key{}", i);
                let v = c.get(&key);
                println!("{}:{}: {}", j, i, *v);
            }
        }
    }

    #[tokio::test]
    async fn test_cache_async() {
        println!("Running test...");
        let c = CacheAsync::new(|x:&String| -> String {
            println!("Generating {}", x);
            let mut y = x.clone();
            y.push_str("@");
            y
        });

        for j in 0..2 {
            for i in 0..10 {
                let key = format!("key{}", i);
                let v = c.get(&key).await;
                println!("{}:{}: {}", j, i, *v);
            }
        }
    }
}
