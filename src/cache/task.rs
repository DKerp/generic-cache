use super::*;



pub(crate) struct CacheTask<K, V, E> {
    /// The store where all the entries are saved at.
    store: BTreeMap<K, CacheEntry<V>>,
    /// The executor.
    executor: Arc<E>,
    /// The total size of all entries inside the cache.
    total_size: Arc<AtomicUsize>,
    /// The total size of the entries managed by this task.
    local_size: usize,
    /// The total number of successfull hits for values kept by this task.
    total_hits: Hitcounter,
}

impl<K, V, E> Drop for CacheTask<K, V, E> {
    fn drop(&mut self) {
        let local_size = self.local_size;

        self.total_size.fetch_update(
            Ordering::SeqCst,
            Ordering::SeqCst,
            move |total_size| total_size.checked_sub(local_size).or(Some(0)),
        ).unwrap(); // Safe because we always return a `Some`, resulting in an `Ok`.
    }
}

impl<K, V, E> CacheTask<K, V, E>
where
    K: Ord + Send + Sync + 'static,
    V: Send + Sync + 'static,
    E: SpawnExt + Send + Sync + 'static,
{
    pub(crate) fn new(
        executor: Arc<E>,
        total_size: Arc<AtomicUsize>,
    ) -> Self {
        let store = BTreeMap::new();

        Self {
            store,
            executor,
            total_size,
            local_size: 0,
            total_hits: 0,
        }
    }

    pub(crate) fn run(self) -> mpsc::UnboundedSender<CacheInnerReq> {
        let (tx, rx) = mpsc::unbounded();

        let executor = Arc::clone(&self.executor);

        executor.spawn(async move {
            self.run_inner(rx).await;
        }).unwrap_or(());

        tx
    }

    async fn run_inner(
        mut self,
        mut req_rx: mpsc::UnboundedReceiver<CacheInnerReq>,
    ) {
        while let Some(req) = req_rx.next().await {
            match req {
                CacheInnerReq::Get(box_key, tx) => {
                    let key: Box<K> = match box_key.downcast() {
                        Ok(key) => key,
                        Err(_) => {
                            let resp = CacheResp::Err(Error::ConvertionFailed);

                            tx.send(resp).unwrap_or(());
                            continue;
                        }
                    };
                    let key: K = *key;

                    let value = self.store.get_mut(&key).map(|entry| entry.get());

                    if value.is_some() {
                        self.total_hits = self.total_hits.checked_add(1).unwrap_or(Hitcounter::MAX);
                    }

                    log::debug!(
                        "CacheTask.get - key type: {}, value type: {}, found: {}, store.len: {}",
                        std::any::type_name::<K>(),
                        std::any::type_name::<V>(),
                        value.is_some(),
                        self.store.len(),
                    );

                    let resp = CacheResp::Get(value);

                    tx.send(resp).unwrap_or(());
                }
                CacheInnerReq::Set(box_key, box_obj, size, config_entry, tx) => {
                    let key: Box<K> = match box_key.downcast() {
                        Ok(key) => key,
                        Err(_) => {
                            let resp = CacheResp::Err(Error::ConvertionFailed);

                            tx.send(resp).unwrap_or(());
                            continue;
                        }
                    };
                    let key: K = *key;

                    let obj: Box<Arc<V>> = match box_obj.downcast() {
                        Ok(obj) => obj,
                        Err(_) => {
                            let resp = CacheResp::Err(Error::ConvertionFailed);

                            tx.send(resp).unwrap_or(());
                            continue;
                        }
                    };
                    let obj: Arc<V> = *obj;

                    // Add the new entry.
                    match self.store.get_mut(&key) {
                        // If it already exists, first substracts the entrie`s current size from
                        // all relevant counters, then add its new size, and finally add the
                        // value to the store.
                        Some(entry) => {
                            let current_size = entry.size();

                            self.total_size.fetch_update(
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                                move |total_size| {
                                    let total_size = total_size.checked_sub(current_size).unwrap_or(0);

                                    total_size.checked_add(size).or(Some(usize::MAX))
                                }
                            ).unwrap(); // Safe because we always return a `Some`, resulting in an `Ok`.

                            self.local_size = self.local_size.checked_sub(current_size).unwrap_or(0);
                            self.local_size = self.local_size.checked_add(size).unwrap_or(usize::MAX);

                            entry.update(obj, size, config_entry);
                        }
                        // If it does not exist simply add its size to all relevant counters
                        // and then add the new entry.
                        None => {
                            let entry = CacheEntry::<V>::new(obj, size, config_entry);

                            self.total_size.fetch_update(
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                                move |total_size| {
                                    total_size.checked_add(size).or(Some(usize::MAX))
                                }
                            ).unwrap(); // Safe because we always return a `Some`, resulting in an `Ok`.

                            self.local_size = self.local_size.checked_add(size).unwrap_or(usize::MAX);

                            self.store.insert(key, entry);
                        }
                    }

                    let resp = CacheResp::Done;

                    tx.send(resp).unwrap_or(());
                }
                CacheInnerReq::Remove(box_key, tx) => {
                    let key: Box<K> = match box_key.downcast() {
                        Ok(key) => key,
                        Err(_) => {
                            let resp = CacheResp::Err(Error::ConvertionFailed);

                            tx.send(resp).unwrap_or(());
                            continue;
                        }
                    };
                    let key: K = *key;

                    let value = match self.store.get_mut(&key) {
                        Some(entry) => {
                            let size = entry.size();

                            self.total_size.fetch_update(
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                                move |total_size| {
                                    total_size.checked_sub(size).or(Some(0))
                                }
                            ).unwrap(); // Safe because we always return a `Some`, resulting in an `Ok`.

                            self.local_size = self.local_size.checked_sub(size).unwrap_or(0);
                            self.total_hits = self.total_hits.checked_sub(entry.total_hits()).unwrap_or(0);

                            let value = entry.get_direct();
                            self.store.remove(&key);
                            Some(value)
                        }
                        None => None,
                    };

                    let resp = CacheResp::Remove(value);

                    tx.send(resp).unwrap_or(());
                }
                CacheInnerReq::Delete(box_key, tx) => {
                    let key: Box<K> = match box_key.downcast() {
                        Ok(key) => key,
                        Err(_) => {
                            let resp = CacheResp::Err(Error::ConvertionFailed);

                            tx.send(resp).unwrap_or(());
                            continue;
                        }
                    };
                    let key: K = *key;

                    if let Some(entry) = self.store.get(&key) {
                        let size = entry.size();

                        self.total_size.fetch_update(
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                            move |total_size| {
                                total_size.checked_sub(size).or(Some(0))
                            }
                        ).unwrap(); // Safe because we always return a `Some`, resulting in an `Ok`.

                        self.local_size = self.local_size.checked_sub(size).unwrap_or(0);
                        self.total_hits = self.total_hits.checked_sub(entry.total_hits()).unwrap_or(0);

                        self.store.remove(&key);
                    }

                    let resp = CacheResp::Done;

                    tx.send(resp).unwrap_or(());
                }
                CacheInnerReq::CheckTimeout(now) => {
                    let begin = self.store.len();

                    let mut total_size_deleted = 0usize;
                    let mut total_hits_deleted: Hitcounter = 0;

                    let start = Instant::now();

                    self.store.retain(|_, entry| {
                        if !entry.is_expired(now) {
                            true
                        } else {
                            total_size_deleted = total_size_deleted.checked_add(entry.size()).unwrap_or(usize::MAX);
                            total_hits_deleted = total_hits_deleted.checked_add(entry.total_hits()).unwrap_or(Hitcounter::MAX);

                            false
                        }
                    });

                    let elapsed = start.elapsed();
                    let dif = begin - self.store.len();

                    log::debug!(
                        "CacheTask for key {} and value {} - timeout check finished. Deleted {} entries with a total size of {} which took {:?}.",
                        std::any::type_name::<K>(), std::any::type_name::<V>(),
                        dif, total_size_deleted, elapsed,
                    );

                    self.total_size.fetch_update(
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                        move |total_size| {
                            total_size.checked_sub(total_size_deleted).or(Some(0))
                        }
                    ).unwrap(); // Safe because we always return a `Some`, resulting in an `Ok`.

                    self.local_size = self.local_size.checked_sub(total_size_deleted).unwrap_or(0);
                    self.total_hits = self.total_hits.checked_sub(total_hits_deleted).unwrap_or(0);
                }
                CacheInnerReq::Cleanup => {
                    // First check that the store is not already empty.
                    if self.store.is_empty() {
                        continue;
                    }

                    // We will delete entries until we have only half of our current size stored.
                    let goal = self.local_size/2;

                    let mut total_size_deleted = 0usize;

                    // We start with the average numer of expected hits. Anything below that amount gets
                    // removed.
                    let mut min_hits = (self.total_hits/(self.store.len() as u128)).min(10); // We add a reseanable minimum.

                    while self.local_size>goal {
                        // let total_size_ref = &mut self.total_size;

                        self.store.retain(|_, entry| {
                            if entry.total_hits()>=min_hits {
                                true
                            } else {
                                total_size_deleted = total_size_deleted.checked_add(entry.size()).unwrap_or(usize::MAX);

                                false
                            }
                        });

                        // Increase the value to delete more entries if necessary in the next round.
                        min_hits = min_hits.checked_add(min_hits/2).unwrap_or(u128::MAX);
                    }

                    self.total_size.fetch_update(
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                        move |total_size| {
                            total_size.checked_sub(total_size_deleted).or(Some(0))
                        }
                    ).unwrap(); // Safe because we always return a `Some`, resulting in an `Ok`.

                    self.local_size = self.local_size.checked_sub(total_size_deleted).unwrap_or(0);

                    // Reset the hit counters.
                    self.total_hits = 0;
                    for (_, entry) in self.store.iter_mut() {
                        entry.reset_total_hits();
                    }
                }
            }
        }
    }
}
