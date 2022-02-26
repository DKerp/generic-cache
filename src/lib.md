A cache which is generic over both the keys as well as the values used. It allows the storing of
__different types__ of values as well as __different types__ of keys inside a __single instance__.

You can roughly imagine it as a nested map with the following structure, where `key` represents
the conrete key of type `K`, and `value` the concrete object instance of type `V`.

```text
<K, V> -> (key: K -> value: Arc<V>)
```

Note that values get automatically wrapped in an [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) by the [`Cache`] itself, so you do not need to wrap your objects in an [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) yourself. You can still do it yourself and add the object through the `set_arc*` methods if you want to save the same object twice under different keys.

# Async runtime required

The different key/value type combinations are managed by seperate tasks, which means you need an async executor which can spawn these tasks. This approach has the advantage of never blocking any thread, which can increase performance if you have a lot of cpu cores available and cache at least as many different types of objects.

You can easily use the [`tokio`](https://docs.rs/tokio) or [`async_std`](https://docs.rs/async_std) runtime by using the [`TokioExecutor`](https://docs.rs/fut-compat/latest/fut_compat/task/struct.TokioExecutor.html) or the [`AsyncStdExecutor`](https://docs.rs/fut-compat/latest/fut_compat/task/struct.AsyncStdExecutor.html) from the [`fut-compat`](https://docs.rs/fut-compat) crate as executors. You can also use any other executor as long as it implements the [`Spawn`](https://docs.rs/futures/latest/futures/task/trait.Spawn.html) trait.

# A quick example

We load a `User` record from the database and create a `Profile` page out of it in a relatively expensive way. Each user can be uniquely identified by either its `id` or its `username`. We save both structures under both keys inside a [`Cache`] instance which gets run by the [`tokio`](https://docs.rs/tokio) runtime.

```rust
use std::sync::Arc;

use generic_cache::Cache;

use fut_compat::task::TokioExecutor;
// If we wanted to use the async_std runtime we would instead import and use the following:
// use fut_compat::task::AsyncStdExecutor;


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

    let cache = Cache::new_with_executor(TokioExecutor::default());

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

    cache.set_arc_with_size(
        user.id,
        Arc::clone(&user),
        0,
    ).await.unwrap();

    cache.set_arc_with_size(
        user.username.clone(),
        Arc::clone(&user),
        0,
    ).await.unwrap();

    cache.set_arc_with_size(
        user.id,
        Arc::clone(&profile),
        0,
    ).await.unwrap();

    cache.set_arc_with_size(
        user.username.clone(),
        Arc::clone(&profile),
        0,
    ).await.unwrap();

    /* Retrieve the values again from the cache. */

    let compare = cache.get::<u64, User>(user.id).await.unwrap().unwrap();
    assert_eq!(user, compare);

    let compare = cache.get::<String, User>(user.username.clone()).await.unwrap().unwrap();
    assert_eq!(user, compare);

    let compare = cache.get::<u64, Profile>(user.id).await.unwrap().unwrap();
    assert_eq!(profile, compare);

    let compare = cache.get::<String, Profile>(user.username.clone()).await.unwrap().unwrap();
    assert_eq!(profile, compare);
}
```
