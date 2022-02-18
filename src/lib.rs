#![doc = include_str!("../README.md")]

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::any::{Any, TypeId};
use std::time::{Instant, Duration};

use futures::channel::{mpsc, oneshot};
use futures::task::{SpawnExt, SpawnError};
use futures::StreamExt;
use futures::select;

use futures_timer::Delay;



mod size;
pub use size::*;

mod builder;
pub use builder::*;

pub mod util;



type BoxObject = Box<dyn Any + Send + Sync + 'static>;
type BoxKey = Box<dyn Any + Send + Sync + 'static>;
type Timestamp = u64;
type Hitcounter = u128;



#[derive(Debug)]
enum CacheReq {
    // key_type, value_type, key, tx
    Get(TypeId, TypeId, BoxKey, oneshot::Sender<CacheResp>),
    // key_type, value_type, key, obj, tx
    Set(TypeId, TypeId, BoxKey, BoxObject, usize, oneshot::Sender<CacheResp>),
    // key_type, value_type, key, tx
    Remove(TypeId, TypeId, BoxKey, oneshot::Sender<CacheResp>),
    // key_type, value_type, key, tx
    Delete(TypeId, TypeId, BoxKey, oneshot::Sender<CacheResp>),
    // key_type, value_type, tx
    Clear(TypeId, TypeId, oneshot::Sender<CacheResp>),
    // key_type, tx
    ClearKey(TypeId, oneshot::Sender<CacheResp>),
    // value_type, tx
    ClearType(TypeId, oneshot::Sender<CacheResp>),
    // tx
    ClearAll(oneshot::Sender<CacheResp>),
    // key_type, value_type, store_tx
    NewStore(TypeId, TypeId, mpsc::UnboundedSender<CacheInnerReq>),
}

#[derive(Debug)]
enum CacheInnerReq {
    DeleteChannel(mpsc::UnboundedSender<CacheInnerResp>),
    Get(BoxKey, Timestamp, oneshot::Sender<CacheResp>),
    Set(BoxKey, Timestamp, BoxObject, usize, oneshot::Sender<CacheResp>),
    Remove(BoxKey, oneshot::Sender<CacheResp>),
    Delete(BoxKey, oneshot::Sender<CacheResp>),
    CheckTimeout(Timestamp),
    Cleanup,
}

#[derive(Debug)]
enum CacheResp {
    Get(Option<BoxObject>),
    Remove(Option<BoxObject>),
    StoreUnknown(BoxKey, BoxObject),
    Done,
    Err(Error),
}

#[derive(Debug)]
enum CacheInnerResp {
    Delete(usize),
    Cleanup(usize),
}



/// The different errors which can be returned by the [`Cache`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Error {
    /// The background task managing the different data stores is no longer available.
    MainUnreachable,
    /// A background task managing a specific data stores can not be reached.
    StoreUnreachable,
    /// The convertion from a boxed trait object to its concrete type failed.
    ConvertionFailed,
    /// Some unexpected condition was met, indicating an implementation error.
    Internal,
    /// Some unexpected condition was met.
    Other,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MainUnreachable => write!(f, "Unreachable (Cache main background task paniced)"),
            Self::StoreUnreachable => write!(f, "StoreUnreachable (A cache store background task paniced)"),
            Self::ConvertionFailed => write!(f, "ConvertionFailed (A boxed trait object could not be converted to its original type)"),
            Self::Internal => write!(f, "Internal (Implementation error)"),
            Self::Other => write!(f, "Other (Unknown error)"),
        }
    }
}

impl std::error::Error for Error {}



/// A cache which is generic over both the keys as well as the values used.
///
/// The cache creates seperate stores ad hoc for each combination of key type and value type used.
/// You can roughly imagine it as a nested map with the following structure, where `key` represents
/// the conrete key you provided, and `value` the concrete object instance that got saved inside the
/// cache.
///
/// ```text
/// <K, V> -> (key: K -> value: Arc<V>)
/// ```
///
/// Note that values get automatically wrapped in an [`Arc`] by the `Cache` itself, so you do not need to wrap your
/// objects in an [`Arc`] yourself.
///
/// You may save objects of different types under the same key type, and you may save objects of the same type
/// under different key types. But in order to retrieve these objects again you must use the same combination
/// of key type and object type which you used to save the objects. Otherwise the `Cache` will return [`None`].
///
/// Furthermore objects are saved and given out as [`Arc`] references, allowing multiple parts of your programm
/// to access them concurrently. If you need to mutate your objects you may wrap them into something that allows
/// interior mutability, like a [`Mutex`](std::sync::Mutex) or a [`RwLock`](std::sync::Mutex). If you want to
/// [delete](Cache::delete) or [remove](Cache::remove) an object from the cache you must do so manually. Note
/// that the object will not get dropped until all [`Arc`] references to it got dropped by your programm.
pub struct Cache<E> {
    sender: mpsc::UnboundedSender<CacheReq>,
    executor: Arc<E>,
}

impl<E> Default for Cache<E>
where
    E: SpawnExt + Default + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Cache<E>
where
    E: SpawnExt + Default + Send + Sync + 'static,
{
    /// Create a new [`Cache`] instance with the default [`Config`] and the default value of the executor.
    pub fn new() -> Self {
        // Safe, because the default Config is permissable.
        Self::build().finish().unwrap()
    }

    /// Try to create a new [`Cache`] instance with the provided [`Config`] and the default value of the executor.
    pub fn new_with_config(config: Config) -> Result<Self, BuildError> {
        Builder::with_executor_and_config(E::default(), config).finish()
    }

    /// Create a cache [`Builder`] with the default value of the executor.
    pub fn build() -> Builder<E> {
        Builder::with_executor(E::default())
    }
}

impl<E> Cache<E>
where
    E: SpawnExt + Send + Sync + 'static
{
    /// Create a new [`Cache`] instance with the default [`Config`] and the provided executor.
    pub fn new_with_executor(executor: E) -> Self {
        // Safe, because the default Config is permissable.
        Self::build_with_executor(executor).finish().unwrap()
    }

    /// Try to create a new [`Cache`] instance with the given executor and the given [`Config`].
    pub fn new_with_executor_and_config(
        executor: E,
        config: Config,
    ) -> Result<Self, BuildError> {
        Builder::with_executor_and_config(executor, config).finish()
    }

    /// Create a cache [`Builder`] with the given executor.
    pub fn build_with_executor(executor: E) -> Builder<E> {
        Builder::with_executor(executor)
    }

    /// Try to retrieve the value of type `V` saved under the given `key` of type `K`.
    pub async fn get<K, V>(
        &self,
        key: K,
    ) -> Result<Option<Arc<V>>, Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let key_id = TypeId::of::<K>();
        let value_id = TypeId::of::<V>();
        let box_key: BoxKey = Box::new(key);

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::Get(key_id, value_id, box_key, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::Get(option) => {
                match option {
                    Some(box_obj) => {
                        let obj: Box<Arc<V>> = box_obj.downcast()
                            .map_err(|_| Error::ConvertionFailed)?;
                        let obj: Arc<V> = *obj;

                        return Ok(Some(obj));
                    }
                    None => return Ok(None),
                }
            }
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }

    /// Try to save the `value` of type `V` under the given `key` of type `K`, replacing any other
    /// value of type `V` currently saved under that `key`.
    ///
    /// The [`Cache`] instance will wrap the `value` inside an [`Arc`] before saving it.
    ///
    /// The size of the `value` gets automatically computed by the [`Size`] trait implementation.
    pub async fn set<K, V>(
        &self,
        key: K,
        value: V,
    ) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Size + Send + Sync + 'static,
    {
        let size = Size::get_size(&value);
        let value = Arc::new(value);

        self.set_arc_with_size::<K, V>(key, value, size).await
    }

    /// Try to save the `value` of type `V` under the given `key` of type `K`, replacing any other
    /// value of type `V` currently saved under that `key`.
    ///
    /// In this variant the `value` is already wrapped in an [`Arc`], so the [`Cache`] instance will
    /// not wrap it itself.
    ///
    /// The size of the `value` gets automatically computed by the [`Size`] trait implementation.
    pub async fn set_arc<K, V>(
        &self,
        key: K,
        value: Arc<V>,
    ) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Size + Send + Sync + 'static,
    {
        // Make sure we take the size of V and not of Arc<V> (note the *).
        let size = Size::get_size(&*value);

        self.set_arc_with_size::<K, V>(key, value, size).await
    }

    /// Try to save the `value` of type `V` under the given `key` of type `K`, replacing any other
    /// value of type `V` currently saved under that `key`.
    ///
    /// The [`Cache`] instance will wrap the `value` inside an [`Arc`] before saving it.
    ///
    /// The `size` of the `value` must be manually handed over.
    pub async fn set_with_size<K, V>(
        &self,
        key: K,
        value: V,
        size: usize,
    ) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let value = Arc::new(value);

        self.set_arc_with_size::<K, V>(key, value, size).await
    }

    /// Try to save the `value` of type `V` under the given `key` of type `K`, replacing any other
    /// value of type `V` currently saved under that `key`.
    ///
    /// In this variant the `value` is already wrapped in an [`Arc`], so the [`Cache`] instance will
    /// not wrap it itself.
    ///
    /// The `size` of the `value` must be manually handed over.
    pub async fn set_arc_with_size<K, V>(
        &self,
        key: K,
        value: Arc<V>,
        size: usize,
    ) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let key_id = TypeId::of::<K>();
        let value_id = TypeId::of::<V>();
        let box_key: BoxKey = Box::new(key);
        let box_obj: BoxObject = Box::new(value);

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::Set(key_id, value_id, box_key, box_obj, size, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::StoreUnknown(box_key, box_obj) => {
                // Create the corresponding store.
                let task = CacheTask::<K, V, E>::new(Arc::clone(&self.executor));

                let inner_req_tx = task.run();

                let req = CacheReq::NewStore(key_id, value_id, inner_req_tx);

                self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

                // Try it again.
                let (tx, rx) = oneshot::channel();

                let req = CacheReq::Set(key_id, value_id, box_key, box_obj, size, tx);

                self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

                let resp = rx.await.map_err(|_| Error::Other)?;

                match resp {
                    CacheResp::StoreUnknown(_, _) => return Err(Error::StoreUnreachable),
                    CacheResp::Done => return Ok(()),
                    CacheResp::Err(err) => return Err(err),
                    _ => return Err(Error::Internal),
                }
            }
            CacheResp::Done => return Ok(()),
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }

    /// Try to remove the value of type `V` saved under the given `key` of type `K`, returning that
    /// value if it existed.
    pub async fn remove<K, V>(
        &self,
        key: K,
    ) -> Result<Option<Arc<V>>, Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let key_id = TypeId::of::<K>();
        let value_id = TypeId::of::<V>();
        let box_key: BoxKey = Box::new(key);

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::Remove(key_id, value_id, box_key, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::Remove(option) => {
                match option {
                    Some(box_obj) => {
                        let obj: Box<Arc<V>> = box_obj.downcast()
                            .map_err(|_| Error::ConvertionFailed)?;
                        let obj: Arc<V> = *obj;

                        return Ok(Some(obj));
                    }
                    None => return Ok(None),
                }
            }
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }

    /// Try to delete the value of type `V` saved under the given `key` of type `K` without returning
    /// that value if it existed.
    pub async fn delete<K, V>(
        &self,
        key: K,
    ) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let key_id = TypeId::of::<K>();
        let value_id = TypeId::of::<V>();
        let box_key: BoxKey = Box::new(key);

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::Delete(key_id, value_id, box_key, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::Done => return Ok(()),
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }

    /// Try to delete all values of type `V` saved under keys of type `K`, leaving values of other types,
    /// as well as values of the same type but saved under another key type, untouched.
    pub async fn clear<K, V>(&self) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let key_id = TypeId::of::<K>();
        let value_id = TypeId::of::<V>();

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::Clear(key_id, value_id, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::Done => return Ok(()),
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }

    /// Try to delete all values of __any__ type saved under keys of type `K`, leaving values saved under
    /// another key type untouched.
    pub async fn clear_key<K>(&self) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
    {
        let key_id = TypeId::of::<K>();

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::ClearKey(key_id, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::Done => return Ok(()),
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }

    /// Try to delete all values of type `V` saved under __any__ type of key, leaving values of other types
    /// untouched.
    pub async fn clear_type<V>(&self) -> Result<(), Error>
    where
        V: Send + Sync + 'static,
    {
        let value_id = TypeId::of::<V>();

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::ClearType(value_id, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::Done => return Ok(()),
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }

    /// Try to delete all values currently saved.
    pub async fn clear_all(&self) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();

        let req = CacheReq::ClearAll(tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::Done => return Ok(()),
            CacheResp::Err(err) => return Err(err),
            _ => return Err(Error::Internal),
        }
    }
}


#[derive(Debug, Clone)]
struct CacheMainTask<E> {
    caches: BTreeMap<(TypeId, TypeId), mpsc::UnboundedSender<CacheInnerReq>>,
    current_ts: Timestamp,
    config: Config,
    executor: Arc<E>,
    total_size: usize,
}

impl<E> CacheMainTask<E>
where
    E: SpawnExt + Send + Sync + 'static
{
    pub fn new_with_config_and_executor(
        config: Config,
        executor: Arc<E>,
    ) -> Self {
        Self {
            caches: BTreeMap::new(),
            current_ts: 0,
            config,
            executor,
            total_size: 0,
        }
    }

    fn run(self) -> mpsc::UnboundedSender<CacheReq> {
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
        /* Create the timestamp update interval */
        let mut timestamp_interval = match self.start_interval(self.config.timestamp_duration) {
            Ok(rx) => rx,
            Err(err) => {
                log::error!("Spawning timestamp update task failed! err: {:?}", err);
                return;
            }
        };

        /* Create the check timeout interval */
        let mut timeout_check_interval = match self.start_interval(self.config.timeout_check_interval) {
            Ok(rx) => rx,
            Err(err) => {
                log::error!("Spawning timestamp update task failed! err: {:?}", err);
                return;
            }
        };

        // Create the channel for receiving delete notifications.
        let (delete_tx, mut delete_rx) = mpsc::unbounded::<CacheInnerResp>();

        loop {
            select! {
                tick = timestamp_interval.next() => {
                    if tick.is_none() {
                        log::error!("CacheMainTask - The timestamp update task paniced! Aborting...");
                        break;
                    }

                    log::debug!("CacheMainTask - Timestamp update triggered.");

                    self.current_ts = match self.current_ts.checked_add(1) {
                        Some(ts) => ts,
                        None => {
                            // Reset the cache, since the timestamps are now meaningless.
                            self.caches.clear();

                            0
                        }
                    };
                }
                tick = timeout_check_interval.next() => {
                    if tick.is_none() {
                        log::error!("CacheMainTask - The check timeout task paniced! Aborting...");
                        break;
                    }

                    log::debug!("CacheMainTask - Timeout check triggered.");

                    let min_last_hit = self.current_ts.checked_sub(self.config.entry_timeout).unwrap_or(0);

                    // Tell all stores to perform a timeout check.
                    // Remove all stores which can not be reached.
                    self.caches.retain(move |_, tx| {
                        let req = CacheInnerReq::CheckTimeout(min_last_hit);

                        tx.unbounded_send(req).is_ok()
                    })
                }
                req = delete_rx.next() => {
                    let req = req.unwrap(); // Safe because we keep a sender half.

                    match req {
                        CacheInnerResp::Delete(amount) => {
                            self.total_size = self.total_size.checked_sub(amount).unwrap_or(0);
                        }
                        CacheInnerResp::Cleanup(total_size) => {
                            self.total_size = match self.total_size.checked_add(total_size) {
                                Some(total_size) => total_size,
                                None => break,
                            };
                        }
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
                                    let ts = self.current_ts;

                                    let req = CacheInnerReq::Get(box_key, ts, tx);

                                    if let Err(err) = cache.unbounded_send(req) {
                                        log::debug!("CacheMainTask - Specific cache could not be contacted.");

                                        self.caches.remove(&id);

                                        if let CacheInnerReq::Get(_box_key, _ts, tx) = err.into_inner() {
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
                        CacheReq::Set(key_id, value_id, box_key, box_obj, size, tx) => {
                            let id = (key_id, value_id);

                            log::debug!("CacheMainTask - Setting a value. Specific cache id: {:?}", id);

                            // We perform the total_size update here, to ensure we can properly handle a counter overflow
                            // and inform the user, which would no longer be possible after giving the tx to the concrete
                            // store task.
                            match self.total_size.checked_add(size) { // TODO
                                Some(total_size) => self.total_size = total_size,
                                None => {
                                    self.caches.clear();

                                    let resp = CacheResp::StoreUnknown(box_key, box_obj);

                                    tx.send(resp).unwrap_or(());
                                    continue;
                                }
                            }

                            match self.caches.get(&id) {
                                Some(cache) => {
                                    let ts = self.current_ts;

                                    let req = CacheInnerReq::Set(box_key, ts, box_obj, size, tx);

                                    if let Err(err) = cache.unbounded_send(req) {
                                        self.caches.remove(&id);

                                        if let CacheInnerReq::Set(box_key, _ts, box_obj, _size, tx) = err.into_inner() {
                                            let resp = CacheResp::StoreUnknown(box_key, box_obj);

                                            tx.send(resp).unwrap_or(());
                                        } else {
                                            log::warn!("CacheMainTask - Returning put response failed due to an internal error!");
                                        }
                                    } else {
                                        // Check if we now have a total size violation, and inform all concrete store tasks about it
                                        // if necessary.
                                        if self.total_size>self.config.max_total_size {
                                            for (_, tx) in self.caches.iter() {
                                                let req = CacheInnerReq::Cleanup;

                                                tx.unbounded_send(req).unwrap_or(());
                                            }

                                            // Reset the counter. The background tasks will inform us of the amount of bytes they
                                            // still carry after they finished their cleanup.
                                            self.total_size = 0;
                                        }

                                        continue;
                                    }
                                }
                                None => {
                                    let resp = CacheResp::StoreUnknown(box_key, box_obj);

                                    tx.send(resp).unwrap_or(());
                                }
                            }

                            // If we reach this point adding the object to the cache has failed.
                            // Subtract the added size again.
                            self.total_size.checked_sub(size).unwrap_or(0);
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
                            let delete_tx = delete_tx.clone();

                            let req = CacheInnerReq::DeleteChannel(delete_tx);

                            if let Err(_) = inner_req_tx.unbounded_send(req) {
                                continue;
                            }

                            let id = (key_id, value_id);

                            self.caches.insert(id, inner_req_tx);
                        }
                    }
                }
            }
        }
    }
}


struct CacheTask<K, V, E> {
    store: BTreeMap<K, Entry<V>>,
    executor: Arc<E>,
    delete_tx: Option<mpsc::UnboundedSender<CacheInnerResp>>,
    total_size: usize,
    total_hits: Hitcounter,
}

impl<K, V, E> CacheTask<K, V, E>
where
    K: Ord + Send + Sync + 'static,
    V: Send + Sync + 'static,
    E: SpawnExt + Send + Sync + 'static,
{
    fn new(executor: Arc<E>) -> Self {
        let store = BTreeMap::new();

        Self {
            store,
            executor,
            delete_tx: None,
            total_size: 0,
            total_hits: 0,
        }
    }

    fn run(self) -> mpsc::UnboundedSender<CacheInnerReq> {
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
                CacheInnerReq::DeleteChannel(delete_tx) => {
                    self.delete_tx = Some(delete_tx);
                }
                CacheInnerReq::Get(box_key, ts, tx) => {
                    let key: Box<K> = match box_key.downcast() {
                        Ok(key) => key,
                        Err(_) => {
                            let resp = CacheResp::Err(Error::ConvertionFailed);

                            tx.send(resp).unwrap_or(());
                            continue;
                        }
                    };
                    let key: K = *key;

                    let value = self.store.get_mut(&key).map(|entry| entry.get(ts));

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
                CacheInnerReq::Set(box_key, ts, box_obj, size, tx) => {
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

                    match self.store.get_mut(&key) {
                        Some(entry) => entry.update(obj, size, ts),
                        None => {
                            let entry = Entry::<V>::new(obj, size, ts);

                            self.store.insert(key, entry);
                        }
                    }

                    let resp = CacheResp::Done;

                    tx.send(resp).unwrap_or(());

                    match self.total_size.checked_add(size) {
                        Some(total_size) => self.total_size = total_size,
                        None => {
                            self.total_size = usize::MAX;

                            break;
                        }
                    }
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
                            self.total_size = self.total_size.checked_sub(size).unwrap_or(0);
                            self.total_hits = self.total_hits.checked_sub(entry.total_hits()).unwrap_or(0);

                            let resp = CacheInnerResp::Delete(size);

                            self.delete_tx.as_ref().unwrap().unbounded_send(resp).unwrap_or(());

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
                        self.total_size = self.total_size.checked_sub(size).unwrap_or(0);
                        self.total_hits = self.total_hits.checked_sub(entry.total_hits()).unwrap_or(0);

                        let resp = CacheInnerResp::Delete(size);

                        self.delete_tx.as_ref().unwrap().unbounded_send(resp).unwrap_or(());

                        self.store.remove(&key);
                    }

                    let resp = CacheResp::Done;

                    tx.send(resp).unwrap_or(());
                }
                CacheInnerReq::CheckTimeout(min_last_hit) => {
                    let begin = self.store.len();

                    let total_size = self.total_size;
                    let total_size_ref = &mut self.total_size;
                    let total_hits_ref = &mut self.total_hits;

                    let now = Instant::now();

                    self.store.retain(move |_, entry| {
                        if entry.last_hit()>=min_last_hit {
                            true
                        } else {
                            *total_size_ref = total_size_ref.checked_sub(entry.size()).unwrap_or(0);
                            *total_hits_ref = total_hits_ref.checked_sub(entry.total_hits()).unwrap_or(0);

                            false
                        }
                    });

                    let elapsed = now.elapsed();
                    let dif = begin - self.store.len();
                    let total_size_dif = total_size-self.total_size;

                    log::debug!(
                        "CacheTask for key {} and value {} - timeout check finished. Deleted {} entries  with a total size of {} which took {:?}.",
                        std::any::type_name::<K>(), std::any::type_name::<V>(),
                        dif, total_size_dif, elapsed,
                    );

                    if total_size_dif>0 {
                        let resp = CacheInnerResp::Delete(total_size_dif);

                        self.delete_tx.as_ref().unwrap().unbounded_send(resp).unwrap_or(());
                    }
                }
                CacheInnerReq::Cleanup => {
                    // First check that the store is not already empty.
                    if self.store.is_empty() {
                        // Just to be sure...
                        self.total_size = 0;
                        self.total_hits = 0;

                        // No need to inform the main task of a zero size.
                        continue;
                    }

                    // We will delete entries until we have only half of our current size stored.
                    let goal = self.total_size/2;

                    // We start with the average numer of expected hits. Anything below that amount gets
                    // removed.
                    let mut min_hits = (self.total_hits/(self.store.len() as u128)).min(10); // We add a reseanable minimum.

                    while self.total_size>goal {
                        let total_size_ref = &mut self.total_size;

                        self.store.retain(move |_, entry| {
                            if entry.total_hits()>=min_hits {
                                true
                            } else {
                                *total_size_ref = total_size_ref.checked_sub(entry.size()).unwrap_or(0);

                                false
                            }
                        });

                        // Increase the value to delete more entries if necessary in the next round.
                        min_hits = match min_hits.checked_add(min_hits/2) {
                            Some(hits) => hits,
                            None => {
                                self.store.clear();
                                self.total_size = 0;
                                break;
                            }
                        }
                    }

                    // Reset the hit counters.
                    self.total_hits = 0;
                    for (_, entry) in self.store.iter_mut() {
                        entry.reset_total_hits();
                    }

                    let resp = CacheInnerResp::Cleanup(self.total_size);

                    self.delete_tx.as_ref().unwrap().unbounded_send(resp).unwrap_or(());
                }
            }
        }

        let resp = CacheInnerResp::Delete(self.total_size);

        self.delete_tx.as_ref().unwrap().unbounded_send(resp).unwrap_or(());
    }
}


struct Entry<V> {
    inner: Arc<V>,
    size: usize,
    last_hit: Timestamp,
    total_hits: Hitcounter,
}

impl<V> Entry<V>
where
    V: Send + Sync + 'static,
{
    pub fn new(
        inner: Arc<V>,
        size: usize,
        last_hit: Timestamp,
    ) -> Self {
        Self {
            inner,
            size,
            last_hit,
            total_hits: 0,
        }
    }

    pub fn update(
        &mut self,
        inner: Arc<V>,
        size: usize,
        last_hit: Timestamp,
    ) {
        self.inner = inner;
        self.size = size;
        self.last_hit = last_hit;
    }

    pub fn get(&mut self, current_ts: Timestamp) -> BoxObject {
        self.total_hits = self.total_hits.checked_add(1).unwrap_or(Hitcounter::MAX);
        self.last_hit = current_ts;

        self.get_direct()
    }

    pub fn get_direct(&self) -> BoxObject {
        let obj = Arc::clone(&self.inner);

        Box::new(obj)
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn last_hit(&self) -> Timestamp {
        self.last_hit
    }

    pub fn total_hits(&self) -> Hitcounter {
        self.total_hits
    }

    pub fn reset_total_hits(&mut self) {
        self.total_hits = 0;
    }
}
