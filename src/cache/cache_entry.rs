use super::*;



pub(crate) struct CacheEntry<V> {
    inner: Arc<V>,
    size: usize,
    to_be_deleted: Option<Instant>,
    last_hit: Instant,
    total_hits: Hitcounter,
    local_config: EntryConfigInner,
}

impl<V> CacheEntry<V>
where
    V: Send + Sync + 'static,
{
    pub fn new(
        inner: Arc<V>,
        size: usize,
        local_config: EntryConfigInner,
    ) -> Self {
        let now = Instant::now();
        let to_be_deleted = now.checked_add(local_config.entry_ttl);

        Self {
            inner,
            size,
            to_be_deleted,
            last_hit: now,
            total_hits: 0,
            local_config,
        }
    }

    pub fn update(
        &mut self,
        inner: Arc<V>,
        size: usize,
        local_config: EntryConfigInner,
    ) {
        let now = Instant::now();
        let to_be_deleted = now.checked_add(local_config.entry_ttl);

        self.inner = inner;
        self.size = size;
        self.to_be_deleted = to_be_deleted;
        self.last_hit = now;
        self.local_config = local_config;
    }

    pub fn get(&mut self) -> BoxObject {
        self.total_hits = self.total_hits.checked_add(1).unwrap_or(Hitcounter::MAX);
        self.last_hit = Instant::now();

        self.get_direct()
    }

    pub fn get_direct(&self) -> BoxObject {
        let obj = Arc::clone(&self.inner);

        Box::new(obj)
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn is_expired(&self, now: Instant) -> bool {
        if let Some(to_be_deleted) = self.to_be_deleted {
            if now>to_be_deleted {
                return true
            }
        }

        match self.last_hit.checked_add(self.local_config.entry_last_hit_timeout) {
            Some(timeout) => return now>timeout,
            None => return false,
        }
    }

    pub fn total_hits(&self) -> Hitcounter {
        self.total_hits
    }

    pub fn reset_total_hits(&mut self) {
        self.total_hits = 0;
    }
}
