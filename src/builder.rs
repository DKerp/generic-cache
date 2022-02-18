use crate::{Cache, CacheMainTask};
use crate::{Timestamp};

use std::time::Duration;
use std::sync::Arc;

use futures::task::SpawnExt;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};



/// The different errors which can be returned by the cache [`Builder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BuildError {
    /// A zero timestamp duration was choosen, which is not permitted.
    InvalidTimestampDuration
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTimestampDuration => write!(f, "InvalidTimestampDuration (A zero timestamp duration was choosen, which is not permitted)"),
        }
    }
}

impl std::error::Error for BuildError {}



/// The configuration options for the [`Cache`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Config {
    pub timestamp_duration: Duration,
    pub entry_timeout: Timestamp,
    pub timeout_check_interval: Duration,
    pub max_total_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            timestamp_duration: Duration::from_secs(1),
            entry_timeout: 60,
            timeout_check_interval: Duration::from_secs(60),
            max_total_size: 100*1024*1024,
        }
    }
}

impl Config {
    pub fn build_with_executor<E>(self, executor: E) -> Result<Cache<E>, BuildError>
    where
        E: SpawnExt + Send + Sync + 'static,
    {
        Builder::with_executor_and_config(executor, self).finish()
    }

    #[cfg(all(feature = "tokio", not(feature = "async-std")))]
    pub fn build(self) -> Result<Cache<TokioSpawn>, BuildError>
    {
        Builder::with_executor_and_config(TokioSpawn::default(), self).finish()
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    pub fn build(self) -> Result<Cache<AsyncStdSpawn>, BuildError>
    {
        Builder::with_executor_and_config(AsyncStdSpawn::default(), self).finish()
    }
}



/// A builder for a [`Cache`] instance.
pub struct Builder<E> {
    executor: E,
    config: Config,
}

#[cfg(all(feature = "tokio", not(feature = "async-std")))]
impl Builder<TokioSpawn> {
    pub fn new() -> Self {
        Self::with_executor(TokioSpawn::default())
    }
}

#[cfg(all(feature = "tokio", not(feature = "async-std")))]
impl Default for Builder<TokioSpawn> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
impl Builder<AsyncStdSpawn> {
    pub fn new() -> Self {
        Self::with_executor(AsyncStdSpawn::default())
    }
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
impl Default for Builder<AsyncStdSpawn> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Builder<E>
where
    E: SpawnExt + Send + Sync + 'static,
{
    pub fn with_executor(executor: E) -> Self {
        Self {
            executor,
            config: Config::default(),
        }
    }

    pub fn with_executor_and_config(
        executor: E,
        config: Config,
    ) -> Self {
        Self {
            executor,
            config,
        }
    }

    pub fn timestamp_duration(&mut self, duration: Duration) -> &mut Self {
        self.config.timestamp_duration = duration;

        self
    }

    pub fn entry_timeout(&mut self, timeout: Timestamp) -> &mut Self {
        self.config.entry_timeout = timeout;

        self
    }

    pub fn timeout_check_interval(&mut self, duration: Duration) -> &mut Self {
        self.config.timeout_check_interval = duration;

        self
    }

    pub fn max_total_size(&mut self, size: usize) -> &mut Self {
        self.config.max_total_size = size;

        self
    }

    pub fn finish(self) -> Result<Cache<E>, BuildError> {
        let executor = Arc::new(self.executor);

        let main = CacheMainTask::new_with_config_and_executor(
            self.config,
            Arc::clone(&executor),
        );

        let sender = main.run();

        Ok(Cache {
            sender,
            executor,
        })
    }
}
