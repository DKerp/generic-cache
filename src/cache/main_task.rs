use super::*;



#[derive(Debug, Clone)]
pub(crate) struct CacheMainTask<E> {
    /// The list of all known key-value types and the connection to their respective task.
    caches: BTreeMap<(TypeId, TypeId), mpsc::UnboundedSender<CacheInnerReq>>, // (key-type, value-type)
    /// The configuration of the cache. Contains all important parameters.
    config: Config,
    /// The executor. Gets used to spawn new background tasks.
    executor: Arc<E>,
    /// The total size of all entries inside the cache.
    total_size: Arc<AtomicUsize>,
}

impl<E> CacheMainTask<E>
where
    E: SpawnExt + Send + Sync + 'static
{
    pub(crate) fn new_with_config_and_executor(
        config: Config,
        executor: Arc<E>,
        total_size: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            caches: BTreeMap::new(),
            config,
            executor,
            total_size,
        }
    }

    pub(crate) fn run(self) -> mpsc::UnboundedSender<CacheReq> {
        let (tx, rx) = mpsc::unbounded();

        let executor = Arc::clone(&self.executor);

        executor.spawn(async move {
            self.run_inner(rx).await;
        }).unwrap_or(());

        tx
    }

    fn start_interval(&self, duration: Duration) -> Result<mpsc::UnboundedReceiver<()>, SpawnError> {
        let (interval_tx, interval_rx) = mpsc::unbounded();

        let executor = Arc::clone(&self.executor);

        executor.spawn(async move {
            let mut interval = Delay::new(duration);

            loop {
                interval.await;

                if let Err(_) = interval_tx.unbounded_send(()) {
                    break;
                }

                interval = Delay::new(duration);
            }
        }).map(move |_| interval_rx)
    }

    async fn run_inner(
        mut self,
        mut req_rx: mpsc::UnboundedReceiver<CacheReq>,
    ) {
        /* Create the check timeout interval */
        let mut timeout_check_interval = match self.start_interval(self.config.timeout_check_interval) {
            Ok(rx) => rx,
            Err(err) => {
                log::error!("Spawning check timeout interval task failed! err: {:?}", err);
                return;
            }
        };

        /* Create the check cleanup thereshold interval */
        let mut cleanup_check_interval = match self.start_interval(self.config.total_size_cleanup_interval) {
            Ok(rx) => rx,
            Err(err) => {
                log::error!("Spawning check cleanup thereshold task failed! err: {:?}", err);
                return;
            }
        };

        loop {
            select! {
                tick = timeout_check_interval.next() => {
                    if tick.is_none() {
                        log::error!("CacheMainTask - The check timeout task paniced! Aborting...");
                        break;
                    }

                    log::debug!("CacheMainTask - Timeout check triggered.");

                    let now = Instant::now();

                    // Tell all stores to perform a timeout check.
                    // Remove all stores which can not be reached.
                    self.caches.retain(move |_, tx| {
                        let req = CacheInnerReq::CheckTimeout(now);

                        tx.unbounded_send(req).is_ok()
                    })
                }
                tick = cleanup_check_interval.next() => {
                    if tick.is_none() {
                        log::error!("CacheMainTask - The check cleanup thereshold task paniced! Aborting...");
                        break;
                    }

                    log::debug!("CacheMainTask - Cleanup check triggered.");

                    let total_size = self.total_size.load(Ordering::SeqCst);

                    if total_size>=self.config.total_size_cleanup {
                        log::debug!(
                            "Total size cleanup thereshold reached - triggering cleanup. total_size: {}, total_size_cleanup: {}",
                            total_size, self.config.total_size_cleanup,
                        );

                        self.caches.retain(|_, tx| {
                            let req = CacheInnerReq::Cleanup;

                            tx.unbounded_send(req).is_ok()
                        })
                    }
                }
                req = req_rx.next() => {
                    let req = match req {
                        Some(req) => req,
                        None => break,
                    };

                    match req {
                        CacheReq::Get(key_id, value_id, box_key, tx) => {
                            let id = (key_id, value_id);

                            match self.caches.get(&id) {
                                Some(cache) => {
                                    let req = CacheInnerReq::Get(box_key, tx);

                                    if let Err(err) = cache.unbounded_send(req) {
                                        log::debug!("CacheMainTask - Specific cache could not be contacted.");

                                        self.caches.remove(&id);

                                        if let CacheInnerReq::Get(_box_key, tx) = err.into_inner() {
                                            let resp = CacheResp::Get(None);

                                            tx.send(resp).unwrap_or(());
                                        } else {
                                            log::warn!("CacheMainTask - Returning get response failed due to an internal error!");
                                        }
                                    }
                                }
                                None => {
                                    log::debug!("CacheMainTask - Specific cache could not be found. id: {:?}", id);

                                    let resp = CacheResp::Get(None);

                                    tx.send(resp).unwrap_or(());
                                }
                            }
                        }
                        CacheReq::Set(key_id, value_id, box_key, box_obj, size, config_entry, tx) => {
                            let id = (key_id, value_id);

                            log::debug!("CacheMainTask - Setting a value. Specific cache id: {:?}", id);

                            match self.caches.get(&id) {
                                Some(cache) => {
                                    let req = CacheInnerReq::Set(box_key, box_obj, size, config_entry, tx);

                                    if let Err(err) = cache.unbounded_send(req) {
                                        self.caches.remove(&id);

                                        if let CacheInnerReq::Set(box_key, box_obj, _size, _config_entry, tx) = err.into_inner() {
                                            let resp = CacheResp::StoreUnknown(box_key, box_obj);

                                            tx.send(resp).unwrap_or(());
                                        } else {
                                            log::warn!("CacheMainTask - Returning put response failed due to an internal error!");
                                        }
                                    }
                                }
                                None => {
                                    let resp = CacheResp::StoreUnknown(box_key, box_obj);

                                    tx.send(resp).unwrap_or(());
                                }
                            }
                        }
                        CacheReq::Remove(key_id, value_id, box_key, tx) => {
                            let id = (key_id, value_id);

                            match self.caches.get(&id) {
                                Some(cache) => {
                                    let req = CacheInnerReq::Remove(box_key, tx);

                                    if let Err(err) = cache.unbounded_send(req) {
                                        self.caches.remove(&id);

                                        if let CacheInnerReq::Remove(_box_key, tx) = err.into_inner() {
                                            let resp = CacheResp::Remove(None);

                                            tx.send(resp).unwrap_or(());
                                        } else {
                                            log::warn!("CacheMainTask - Returning remove response failed due to an internal error!");
                                        }
                                    }
                                }
                                None => {
                                    let resp = CacheResp::Remove(None);

                                    tx.send(resp).unwrap_or(());
                                }
                            }
                        }
                        CacheReq::Delete(key_id, value_id, box_key, tx) => {
                            let id = (key_id, value_id);

                            match self.caches.get(&id) {
                                Some(cache) => {
                                    let req = CacheInnerReq::Delete(box_key, tx);

                                    if let Err(err) = cache.unbounded_send(req) {
                                        self.caches.remove(&id);

                                        if let CacheInnerReq::Delete(_box_key, tx) = err.into_inner() {
                                            let resp = CacheResp::Err(Error::StoreUnreachable);

                                            tx.send(resp).unwrap_or(());
                                        } else {
                                            log::warn!("CacheMainTask - Returning delete response failed due to an internal error!");
                                        }
                                    }
                                }
                                None => {
                                    let resp = CacheResp::Err(Error::StoreUnreachable);

                                    tx.send(resp).unwrap_or(());
                                }
                            }
                        }
                        CacheReq::Clear(key_id, value_id, tx) => {
                            let id = (key_id, value_id);

                            self.caches.remove(&id);

                            let resp = CacheResp::Done;

                            tx.send(resp).unwrap_or(());
                        }
                        CacheReq::ClearKey(key_id, tx) => {
                            self.caches.retain(move |&(key_id_probe, _), _| key_id!=key_id_probe);

                            let resp = CacheResp::Done;

                            tx.send(resp).unwrap_or(());
                        }
                        CacheReq::ClearType(value_id, tx) => {
                            self.caches.retain(move |&(_, value_id_probe), _| value_id!=value_id_probe);

                            let resp = CacheResp::Done;

                            tx.send(resp).unwrap_or(());
                        }
                        CacheReq::ClearAll(tx) => {
                            self.caches.clear();

                            let resp = CacheResp::Done;

                            tx.send(resp).unwrap_or(());
                        }
                        CacheReq::NewStore(key_id, value_id, inner_req_tx) => {
                            let id = (key_id, value_id);

                            self.caches.insert(id, inner_req_tx);
                        }
                    }
                }
            }
        }
    }
}
