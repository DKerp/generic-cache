use crate::Size;
use crate::entry::{Entry, EntryConfigInner};

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::any::{Any, TypeId};
use std::time::{Instant, Duration};

use futures::channel::{mpsc, oneshot};
use futures::task::{SpawnExt, SpawnError};
use futures::StreamExt;
use futures::select;

use futures_timer::Delay;



// The configuration and cache builder objects.
mod config;
pub use config::*;

// The main background task, managing all concrete cache storage tasks.
mod main_task;
use main_task::*;

// The storage task for a particular key-type/value-type combination.
mod task;
use task::*;

// The internal object containing a concrete cache entry.
mod cache_entry;
use cache_entry::*;



type BoxObject = Box<dyn Any + Send + Sync + 'static>;
type BoxKey = Box<dyn Any + Send + Sync + 'static>;
type Hitcounter = u128;



#[derive(Debug)]
pub(crate) enum CacheReq {
    // key_type, value_type, key, tx
    Get(TypeId, TypeId, BoxKey, oneshot::Sender<CacheResp>),
    // key_type, value_type, key, obj, tx
    Set(TypeId, TypeId, BoxKey, BoxObject, usize, EntryConfigInner, oneshot::Sender<CacheResp>),
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
pub(crate) enum CacheInnerReq {
    Get(BoxKey, oneshot::Sender<CacheResp>),
    Set(BoxKey, BoxObject, usize, EntryConfigInner, oneshot::Sender<CacheResp>),
    Remove(BoxKey, oneshot::Sender<CacheResp>),
    Delete(BoxKey, oneshot::Sender<CacheResp>),
    CheckTimeout(Instant),
    Cleanup,
}

#[derive(Debug)]
pub(crate) enum CacheResp {
    Get(Option<BoxObject>),
    Remove(Option<BoxObject>),
    StoreUnknown(BoxKey, BoxObject),
    Done,
    Err(Error),
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
    /// The maximum size of the cache has been reached, so adding more objects is not possible.
    MaxSizeReached,
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
            Self::MaxSizeReached => write!(f, "MaxSizeReached (Adding the object was not possible duo to memory exhaustion protection)"),
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
/// objects in an [`Arc`] yourself.You can still do it yourself
/// and add the object through the `set_arc*` methods if you want to save the same object twice under different keys.
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
    config: Config,
    executor: Arc<E>,
    /// The total size of all entries inside the cache.
    total_size: Arc<AtomicUsize>,
}

impl<E> Clone for Cache<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            config: self.config,
            executor: Arc::clone(&self.executor),
            total_size: Arc::clone(&self.total_size),
        }
    }
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
    /// Create a new [`Cache`] instance with the default configuration and the default value of the executor.
    pub fn new() -> Self {
        Self::build().finish()
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
    /// Create a new [`Cache`] instance with the default configuration and the provided executor.
    pub fn new_with_executor(executor: E) -> Self {
        Self::build_with_executor(executor).finish()
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
        let entry = Entry::<K, V>::new_with_arc(key, value);

        self.set_entry_with_size::<K, V>(entry, size).await
    }

    pub async fn set_entry<K, V>(
        &self,
        entry: Entry<K, V>,
    ) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Size + Send + Sync + 'static,
    {
        // Take the size as configured by the entry or calculate it if not set.
        // Make sure we take the size of V and not of Arc<V> (note the *).
        let size = entry.size().unwrap_or_else(|| Size::get_size(&*entry.value()));

        self.set_entry_with_size::<K, V>(entry, size).await
    }

    pub async fn set_entry_with_size<K, V>(
        &self,
        entry: Entry<K, V>,
        size: usize,
    ) -> Result<(), Error>
    where
        K: Ord + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        // Load the current size of the cache.
        let total_size = self.total_size.load(Ordering::SeqCst);
        let total_size = total_size.checked_add(size).ok_or_else(|| Error::MaxSizeReached)?;

        // Abort if adding the new object would exceed the configured limit.
        // We assume the object does not exist yet. This means this may be stricter then necessary.
        if total_size>=self.config.max_total_size {
            return Err(Error::MaxSizeReached);
        }

        let (key, value, config_entry) = entry.into_parts();

        let key_id = TypeId::of::<K>();
        let value_id = TypeId::of::<V>();
        let box_key: BoxKey = Box::new(key);
        let box_obj: BoxObject = Box::new(value);

        let entry_ttl = config_entry.entry_ttl.unwrap_or(self.config.entry_ttl);
        let entry_last_hit_timeout = config_entry.entry_last_hit_timeout.unwrap_or(self.config.entry_last_hit_timeout);

        let config_entry = EntryConfigInner {
            entry_ttl,
            entry_last_hit_timeout,
        };

        let (tx, rx) = oneshot::channel();

        let req = CacheReq::Set(key_id, value_id, box_key, box_obj, size, config_entry, tx);

        self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

        let resp = rx.await.map_err(|_| Error::Other)?;

        match resp {
            CacheResp::StoreUnknown(box_key, box_obj) => {
                // Create the corresponding store.
                let task = CacheTask::<K, V, E>::new(
                    Arc::clone(&self.executor),
                    Arc::clone(&self.total_size),
                );

                let inner_req_tx = task.run();

                let req = CacheReq::NewStore(key_id, value_id, inner_req_tx);

                self.sender.unbounded_send(req).map_err(|_| Error::MainUnreachable)?;

                // Try it again.
                let (tx, rx) = oneshot::channel();

                let req = CacheReq::Set(key_id, value_id, box_key, box_obj, size, config_entry, tx);

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
