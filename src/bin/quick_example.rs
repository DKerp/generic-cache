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
    pretty_env_logger::init();

    /* Initialize the cache. */

    // We implicitely use the default config.
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

    // This is not necessary, but makes it easier to save the same object
    // twice under different keys without cloning.
    let user = Arc::new(user);
    let profile = Arc::new(profile);

    /* Save the values inside the cache. */

    // Save the user record indexed unter its id.
    cache.set_arc_with_size(
        user.id, // The key (u64)
        Arc::clone(&user), // The value (Arc<User>)
        0, // The size in bytes. Should be adjusted as appropriate.
    ).await.unwrap();

    // Save the user record indexed unter its username.
    cache.set_arc_with_size(
        user.username.clone(), // The key (String)
        Arc::clone(&user), // The value (Arc<User>)
        0, // The size in bytes. Should be adjusted as appropriate.
    ).await.unwrap();

    // Save the generated profile indexed unter the user's id.
    cache.set_arc_with_size(
        user.id, // The key (u64)
        Arc::clone(&profile), // The value (Arc<Profile>)
        0, // The size in bytes. Should be adjusted as appropriate.
    ).await.unwrap();

    // Save the generated profile indexed unter the user's username.
    cache.set_arc_with_size(
        user.username.clone(), // The key (String)
        Arc::clone(&profile), // The value (Arc<Profile>)
        0, // The size in bytes. Should be adjusted as appropriate.
    ).await.unwrap();

    /* Retrieve the values again from the cache. */

    // Note: We use two `unwrap` because the return type is `Result<Option<Arc<T>>, Error>`.
    // Note: We are saving an object wrapped in an Arc, so the we get back a double Arc
    // (e.g. Arc<Arc<User>>).

    // Find the user by its id.
    // Fully qualified syntax.
    let compare = cache.get::<u64, User>(user.id).await.unwrap().unwrap();
    assert_eq!(user, compare);

    // Now find it by its username.
    // Fully qualified syntax.
    let compare = cache.get::<String, User>(user.username.clone()).await.unwrap().unwrap();
    assert_eq!(user, compare);

    // Find the rendered profile page by the user`s id.
    // Fully qualified syntax with implicit key type.
    let compare = cache.get::<_, Profile>(user.id).await.unwrap().unwrap();
    assert_eq!(profile, compare);

    // Find the rendered profile page by the user`s username.
    // Implicit syntax. Note that we must annotate the Arc.
    let compare: Arc<Profile> = cache.get(user.username.clone()).await.unwrap().unwrap();
    assert_eq!(profile, compare);
}
