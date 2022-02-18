A cache which is generic over both the keys as well as the values used. It allows the storing of
__different types__ of values as well as __different types__ of keys inside a __single instance__.

You can roughly imagine it as a nested map with the following structure, where `key` represents
the conrete key of type `K`, and `value` the concrete object instance of type `V`.

```text
<K, V> -> (key: K -> value: Arc<V>)
```

Note that values get automatically wrapped in an [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html)
by the [`Cache`](crate::Cache) itself, so you do not need to wrap your
objects in an [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) yourself.

# Async runtime required

The different key/value type combinations are managed by seperate tasks, which means you need an async runtime which can
spawn these tasks. This approach has the advantage of never blocking any thread.

This crate provides support for the [`tokio`](https://docs.rs/tokio) as well as the [`async_std`](https://docs.rs/async_std)
runtime out of the box. You can use the [`TokioSpawn`](crate::util::TokioSpawn) and the [`AsyncStdSpawn`](crate::util::AsyncStdSpawn)
as executors in order to use the respective runtime. You can also use any other executor as long as it implements the [`Spawn`](futures::task::Spawn) trait.

# A quick example

We load a `User` record from the database and create a `Profile` page out of it in a relatively expensive way.
Each user can be uniquely identified by either its `id` or its `username`. We save both structures under both
keys inside a [`Cache`](crate::Cache) instance which gets run by the [`tokio`](https://docs.rs/tokio) runtime.

```rust
use std::sync::Arc;

use generic_cache::Cache;
use generic_cache::util::TokioSpawn;
// If we wanted to use the async_std runtime we would instead import and use the following:
// use generic_cache::util::AsyncStdSpawn;


#[derive(Debug, PartialEq, Eq)]
struct User {
    pub id: u64,
    pub username: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Profile {
    pub html: Vec<u8>,
    pub html_gzipped: Vec<u8>
}


// Can be replaced with #[async_std::main] if necessary.
#[tokio::main]
async fn main() {
    /* Initialize the cache. */

    let cache = Cache::new_with_executor(TokioSpawn::default());

    /* Prepare the values to be cached. */

    let user = User {
        id: 123,
        username: "Adam".into(),
    };

    let profile = Profile {
        html: vec![1u8; 1024],
        html_gzipped: vec![2u8; 128],
    };

    let user = Arc::new(user);
    let profile = Arc::new(profile);

    /* Save the values inside the cache. */

    cache.set_with_size(
        user.id,
        Arc::clone(&user),
        0,
    ).await.unwrap();

    cache.set_with_size(
        user.username.clone(),
        Arc::clone(&user),
        0,
    ).await.unwrap();

    cache.set_with_size(
        user.id,
        Arc::clone(&profile),
        0,
    ).await.unwrap();

    cache.set_with_size(
        user.username.clone(),
        Arc::clone(&profile),
        0,
    ).await.unwrap();

    /* Retrieve the values again from the cache. */

    let compare = cache.get::<u64, Arc<User>>(user.id).await.unwrap().unwrap();
    assert_eq!(user, *compare);

    let compare = cache.get::<String, Arc<User>>(user.username.clone()).await.unwrap().unwrap();
    assert_eq!(user, *compare);

    let compare = cache.get::<u64, Arc<Profile>>(user.id).await.unwrap().unwrap();
    assert_eq!(profile, *compare);

    let compare = cache.get::<String, Arc<Profile>>(user.username.clone()).await.unwrap().unwrap();
    assert_eq!(profile, *compare);
}
```
