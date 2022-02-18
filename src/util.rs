use futures::task::{Spawn, SpawnError, FutureObj};



/// A helper macro which creates an enum for using multiple types as the same type inside the cache.
///
/// # Example
///
/// Create an enum called `Name` which contains the 3 variants `String`, `Bool` and `Char`, where each variant
/// contains an object of type [`String`], [`bool`] and [`char`] respectively.
///
/// ```
/// use generic_cache::create_enum;
///
/// create_enum!(
///     Name;
///     String - String,
///     Bool - bool,
///     Char - char
/// );
/// ```
///
/// The expended version would look like this:
/// ```
/// pub enum Name {
///     String(String),
///     Bool(bool),
///     Char(char),
/// }
///
/// impl From<String> for Name {
///     fn from(obj: String) -> Self {
///         Self::String(obj)
///     }
/// }
///
/// impl From<bool> for Name {
///     fn from(obj: bool) -> Self {
///         Self::Bool(obj)
///     }
/// }
///
/// impl From<char> for Name {
///     fn from(obj: char) -> Self {
///         Self::Char(obj)
///     }
/// }
///
/// impl TryFrom<Name> for String {
///     type Error = ();
///
///     fn try_from(obj_enum: Name) -> Result<Self, Self::Error> {
///         if let Name::String(obj) = obj_enum {
///             return Ok(obj);
///         }
///
///         Err(())
///     }
/// }
///
/// impl TryFrom<Name> for bool {
///     type Error = ();
///
///     fn try_from(obj_enum: Name) -> Result<Self, Self::Error> {
///         if let Name::Bool(obj) = obj_enum {
///             return Ok(obj);
///         }
///
///         Err(())
///     }
/// }
///
/// impl TryFrom<Name> for char {
///     type Error = ();
///
///     fn try_from(obj_enum: Name) -> Result<Self, Self::Error> {
///         if let Name::Char(obj) = obj_enum {
///             return Ok(obj);
///         }
///
///         Err(())
///     }
/// }
/// ```
#[macro_export]
macro_rules! create_enum {
    ($enum_name:ident; $($variant_name:ident - $variant_type:ty),*) => {
        pub enum $enum_name {
            $($variant_name($variant_type),)*
        }

        $(impl From<$variant_type> for $enum_name {
            fn from(obj: $variant_type) -> Self {
                Self::$variant_name(obj)
            }
        }

        impl TryFrom<$enum_name> for $variant_type {
            type Error = ();

            fn try_from(obj_enum: $enum_name) -> Result<Self, Self::Error> {
                if let $enum_name::$variant_name(obj) = obj_enum {
                    return Ok(obj);
                }

                Err(())
            }
        })*
    }
}

// create_struct_enum!(
//     Name;
//     String - String,
//     Bool - bool,
//     Char - char
// );


#[cfg(feature = "tokio")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "tokio")))]
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TokioSpawn {}

#[cfg(feature = "tokio")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "tokio")))]
impl Spawn for TokioSpawn {
    fn spawn_obj(
        &self,
        future: FutureObj<'static, ()>
    ) -> Result<(), SpawnError> {
        tokio::spawn(future);

        Ok(())
    }
}

#[cfg(feature = "async-std")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "async-std")))]
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AsyncStdSpawn {}

#[cfg(feature = "async-std")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "async-std")))]
impl Spawn for AsyncStdSpawn {
    fn spawn_obj(
        &self,
        future: FutureObj<'static, ()>
    ) -> Result<(), SpawnError> {
        async_std::task::spawn(future);

        Ok(())
    }
}

// /// Internal struct for the builder, used to indicate that a valid executor has not yet been added.
// struct NoExecutor {}
//
// impl Spawn for NoExecutor {
//     fn spawn_obj(
//         &self,
//         _future: FutureObj<'static, ()>
//     ) -> Result<(), SpawnError> {
//         panic!("This is not a valid executor.");
//     }
// }
