use super::*;



/// The configuration options for the [`Cache`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Config {
    /// The maximum time to pass between hits on a particular entry after which it
    /// will get removed from the cache.
    ///
    /// Default: __60 secs__
    pub entry_last_hit_timeout: Duration,
    /// The maximum time for each entry to stay inside the cache.
    ///
    /// Default: __300 secs__
    pub entry_ttl: Duration,
    /// The interval at which a timeout check is performed on all entries in the cache.
    ///
    /// Default: __60 secs__
    pub timeout_check_interval: Duration,
    /// The thereshold in bytes after which a cleanup based on the number of total
    /// hits on each entry in the cache gets performed.
    ///
    /// Default: __50 MB__
    pub total_size_cleanup: usize,
    /// The interval at which to check if the cleanup thereshold is currently exceeded,
    /// leading to a cleanup being triggered if so.
    ///
    /// Default: __10 secs__
    pub total_size_cleanup_interval: Duration,
    /// The total size in bytes that values saved inside the cache are allowed to occupy.
    ///
    /// Default: __100 MB__
    pub max_total_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            entry_last_hit_timeout: Duration::from_secs(60),
            entry_ttl: Duration::from_secs(300),
            timeout_check_interval: Duration::from_secs(60),
            total_size_cleanup: 50*1024*1024,
            total_size_cleanup_interval: Duration::from_secs(10),
            max_total_size: 100*1024*1024,
        }
    }
}



/// A builder for a [`Cache`] instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Builder<E> {
    executor: E,
    config: Config,
}

#[cfg(all(feature = "tokio", not(feature = "async-std")))]
impl Builder<TokioExecutor> {
    /// Initializes a new [`Builder`] instance with the [`TokioExecutor`] executor and the default configuration.
    pub fn new() -> Self {
        Self::with_executor(TokioExecutor::default())
    }
}

#[cfg(all(feature = "tokio", not(feature = "async-std")))]
impl Default for Builder<TokioExecutor> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
impl Builder<AsyncStdExecutor> {
    /// Initializes a new [`Builder`] instance with the [`AsyncStdExecutor`] executor and the default configuration.
    pub fn new() -> Self {
        Self::with_executor(AsyncStdExecutor::default())
    }
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
impl Default for Builder<AsyncStdExecutor> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Builder<E>
where
    E: SpawnExt + Send + Sync + 'static,
{
    /// Initializes a new [`Builder`] instance with the given executor and the default configuration.
    pub fn with_executor(executor: E) -> Self {
        Self {
            executor,
            config: Config::default(),
        }
    }

    /// The maximum time to pass between hits on a particular entry after which it
    /// will get removed from the cache.
    ///
    /// Default: __60 secs__
    pub fn entry_last_hit_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.config.entry_last_hit_timeout = timeout;

        self
    }

    /// The maximum time for each entry to stay inside the cache.
    ///
    /// Default: __300 secs__
    pub fn entry_ttl(&mut self, timeout: Duration) -> &mut Self {
        self.config.entry_ttl = timeout;

        self
    }

    /// The interval at which a timeout check is performed on all entries in the cache.
    ///
    /// Default: __60 secs__
    pub fn timeout_check_interval(&mut self, duration: Duration) -> &mut Self {
        self.config.timeout_check_interval = duration;

        self
    }

    /// The thereshold in bytes after which a cleanup based on the number of total
    /// hits on each entry in the cache gets performed.
    ///
    /// Default: __50 MB__
    pub fn total_size_cleanup(&mut self, size: usize) -> &mut Self {
        self.config.total_size_cleanup = size;

        self
    }

    /// The interval at which to check if the cleanup thereshold is currently exceeded,
    /// leading to a cleanup being triggered if so.
    ///
    /// Default: __10 secs__
    pub fn total_size_cleanup_interval(&mut self, duration: Duration) -> &mut Self {
        self.config.total_size_cleanup_interval = duration;

        self
    }

    /// The total size in bytes that values saved inside the cache are allowed to occupy.
    ///
    /// Default: __100 MB__
    pub fn max_total_size(&mut self, size: usize) -> &mut Self {
        self.config.max_total_size = size;

        self
    }

    /// Creates a [`Cache`] instance with the configured values.
    pub fn finish(self) -> Cache<E> {
        let executor = Arc::new(self.executor);
        let total_size = Arc::new(AtomicUsize::new(0));

        let main = CacheMainTask::new_with_config_and_executor(
            self.config,
            Arc::clone(&executor),
            Arc::clone(&total_size),
        );

        let sender = main.run();

        let config = self.config;

        Cache {
            sender,
            config,
            executor,
            total_size,
        }
    }
}
