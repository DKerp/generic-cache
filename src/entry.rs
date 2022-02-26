use std::time::Duration;
use std::sync::Arc;



/// A new entry to be added to the [`Cache`](crate::Cache).
pub struct Entry<K, V> {
    key: K,
    value: Arc<V>,
    size: Option<usize>,
    config: EntryConfig,
}

impl<K, V> Entry<K, V>
where
    K: Ord + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    pub fn new(
        key: K,
        value: V,
    ) -> Self {
        let value = Arc::new(value);

        Self::new_with_arc(key, value)
    }

    pub fn new_with_arc(
        key: K,
        value: Arc<V>,
    ) -> Self {
        Self {
            key,
            value,
            size: None,
            config: EntryConfig::default(),
        }
    }

    pub(crate) fn into_parts(self) -> (K, Arc<V>, EntryConfig) {
        (self.key, self.value, self.config)
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn key_mut(&mut self) -> &mut K {
        &mut self.key
    }

    pub fn set_key(&mut self, key: impl Into<K>) {
        self.key = key.into();
    }

    pub fn value(&self) -> &Arc<V> {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut Arc<V> {
        &mut self.value
    }

    pub fn set_value(&mut self, value: impl Into<V>) {
        self.value = Arc::new(value.into());
    }

    pub fn set_value_arc(&mut self, value: Arc<V>) {
        self.value = value
    }


    pub fn size(&self) -> Option<usize> {
        self.size
    }

    pub fn size_mut(&mut self) -> &mut Option<usize> {
        &mut self.size
    }

    pub fn set_size(&mut self, size: impl Into<usize>) {
        self.size = Some(size.into());
    }


    pub fn last_hit_timeout(&self) -> &Option<Duration> {
        &self.config.entry_last_hit_timeout
    }

    pub fn last_hit_timeout_mut(&mut self) -> &mut Option<Duration> {
        &mut self.config.entry_last_hit_timeout
    }

    pub fn set_last_hit_timeout(&mut self, duration: impl Into<Duration>) {
        self.config.entry_last_hit_timeout = Some(duration.into());
    }

    pub fn ttl(&self) -> &Option<Duration> {
        &self.config.entry_ttl
    }

    pub fn ttl_mut(&mut self) -> &mut Option<Duration> {
        &mut self.config.entry_ttl
    }

    pub fn set_ttl(&mut self, duration: impl Into<Duration>) {
        self.config.entry_ttl = Some(duration.into());
    }
}



#[derive(Debug, Clone, Copy)]
pub(crate) struct EntryConfigInner {
    /// The maximum time to pass between hits on a particular entry after which it
    /// will get removed from the cache.
    pub entry_last_hit_timeout: Duration,
    /// The maximum time for each entry to stay inside the cache.
    pub entry_ttl: Duration,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EntryConfig {
    /// The maximum time to pass between hits on a particular entry after which it
    /// will get removed from the cache.
    pub entry_last_hit_timeout: Option<Duration>,
    /// The maximum time for each entry to stay inside the cache.
    pub entry_ttl: Option<Duration>,
}

impl Default for EntryConfig {
    fn default() -> Self {
        Self {
            entry_last_hit_timeout: None,
            entry_ttl: None,
        }
    }
}
