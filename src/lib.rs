use std::error::Error;
use std::io::Read;
use std::sync::{Arc, RwLock};
use std::hash::Hash;
use std::{fs, thread};
use std::time::Duration;
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

/// FromWatchedFile is a struct that reads a file and watches for changes to the file.
/// When the file changes, the struct will reload the file and update the value in the background.
/// This struct is useful for reloading configuration files or other files that are read frequently.
/// It is thread safe. Note: Each FromWatchedFile spawns a new thread to watch the file do not use too many of them!
pub struct FromWatchedFile<T> {
    value: Arc<RwLock<Arc<Option<T>>>>,
}

impl<T> FromWatchedFile<T>
where
    T: Send + Sync + 'static,
{
    /// Read bytes from file
    fn read_file(file_path: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut file = fs::File::open(file_path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        Ok(contents)
    }

    /// Create a new FromWatchedFile struct and spawn a new thread with given interval and converter function.
    /// Converter function converts a slice of bytes to the desired type.
    /// 
    /// The file will be check based on interval. On change detected, the parser will be used
    /// to convert the file content to desired type.
    /// 
    /// You can get the latest copy using the `get` method.
    /// 
    /// Upon initialization, the first copy will be constructed.
    /// 
    /// The code never fails. If the file gone missing, or the file is not readable, the value will be None.
    pub fn new<F>(file_path: &str, parser: F, interval: Duration) -> Self
        where
        F: Fn(&[u8]) -> T + Send + Sync + 'static,
    {
        let current_content = Self::read_file(file_path);

        // initial loading
        let value = match current_content {
            Ok(content) => {
                Arc::new(RwLock::new(Arc::new(Some(parser(&content)))))
            },
            Err(_) => {
                Arc::new(RwLock::new(Arc::new(None)))
            }
        };
        let value_clone = value.clone();
        let file_path = file_path.to_string();


        let mut last_modified = fs::metadata(&file_path).ok().and_then(|m| m.modified().ok());
        thread::spawn(move || {
            loop {
                thread::sleep(interval);
                let metadata = fs::metadata(&file_path).ok();
                let modified = metadata.and_then(|m| m.modified().ok());

                if modified != last_modified {
                    let content = Self::read_file(file_path.as_str());
                    match content {
                        Ok(bytes) => {
                            let parsed_value = parser(&bytes);
                            let mut w = value_clone.write().unwrap();
                        
                            *w = Arc::new(Some(parsed_value));
                            last_modified = modified;
                        },
                        Err(_) => {
                            // file read error - silently ignore
                        }
                    }
                }
            }
        });

        Self {
            value,
        }
    }


    /// Get the desired converted type from the file
    /// If the file become not readable, it will return the last good copy.
    pub fn get<'a>(&'a self) -> Arc<Option<T>>
        where T: Clone,
    {
        let result = self.value.read().unwrap();
        let clone = Arc::clone(&*result);
        return clone;
    }
}

#[cfg(test)]
mod tests {
    use thread::sleep;

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

    #[test]
    fn test_load_file() {
        fn file_to_string(bytes: &[u8]) -> String {
            String::from_utf8_lossy(bytes).to_string()
        }
    
        // Initialize the FromWatchedFile struct
        let cfg: FromWatchedFile<String> = FromWatchedFile::new("config.json", file_to_string, Duration::from_secs(5));
    
        for _i in 0..100 {
            // Access the current value using get_ref()
            let config = cfg.get();
            match config.as_ref() {
                Some(c) => println!("Config: {}", c),
                None => println!("Config not loaded yet"),
            }   
            // Sleep for 5 seconds before checking the config again
            sleep(Duration::from_secs(1));
        }
    }
}