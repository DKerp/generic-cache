use std::sync::{Arc, Mutex, RwLock};
use std::collections::{
    BTreeMap,
    BTreeSet,
    BinaryHeap,
    HashMap,
    HashSet,
    LinkedList,
    VecDeque,
};
use std::num::{
    NonZeroI8,
    NonZeroI16,
    NonZeroI32,
    NonZeroI64,
    NonZeroI128,
    NonZeroIsize,
    NonZeroU8,
    NonZeroU16,
    NonZeroU32,
    NonZeroU64,
    NonZeroU128,
    NonZeroUsize,
};
use std::convert::Infallible;
use std::borrow::Cow;
use std::rc::Rc;
use std::marker::{PhantomData, PhantomPinned};



/// Determine the size in bytes an object occupies inside RAM. It may be an approximation.
pub trait Size: Sized {
    /// Determines how may bytes this object occupies inside the stack.
    ///
    /// The default implementation uses [std::mem::size_of] and should work for almost if not all types.
    fn get_stack_size() -> usize {
        std::mem::size_of::<Self>()
    }

    /// Determines how many bytes this object occupies inside the heap.
    ///
    /// The default implementation returns 0, assuming the object is fully allocated on the stack.
    /// It must be adjusted as appropriate for objects which hold data inside the heap.
    fn get_heap_size(&self) -> usize {
        0
    }

    /// Determines the total size of the object.
    ///
    /// The default implementation simply adds up the result of the other two functions and is not meant
    /// to be changed.
    fn get_size(&self) -> usize {
        Self::get_stack_size() + Size::get_heap_size(self)
    }
}



impl Size for () {}
impl Size for bool {}
impl Size for u8 {}
impl Size for u16 {}
impl Size for u32 {}
impl Size for u64 {}
impl Size for u128 {}
impl Size for usize {}
impl Size for NonZeroU8 {}
impl Size for NonZeroU16 {}
impl Size for NonZeroU32 {}
impl Size for NonZeroU64 {}
impl Size for NonZeroU128 {}
impl Size for NonZeroUsize {}
impl Size for i8 {}
impl Size for i16 {}
impl Size for i32 {}
impl Size for i64 {}
impl Size for i128 {}
impl Size for isize {}
impl Size for NonZeroI8 {}
impl Size for NonZeroI16 {}
impl Size for NonZeroI32 {}
impl Size for NonZeroI64 {}
impl Size for NonZeroI128 {}
impl Size for NonZeroIsize {}
impl Size for f32 {}
impl Size for f64 {}
impl Size for char {}

impl Size for Infallible {}
impl<T> Size for PhantomData<T> {}
impl Size for PhantomPinned {}



impl<'a, T> Size for Cow<'a, T>
where
    T: ToOwned + Size,
    <T as ToOwned>::Owned: Size,
{
    fn get_heap_size(&self) -> usize {
        match self {
            Self::Borrowed(borrowed) => Size::get_heap_size(borrowed),
            Self::Owned(owned) => Size::get_heap_size(owned),
        }
    }
}



macro_rules! impl_size_set {
    ($name:ident) => {
        impl<T> Size for $name<T> where T: Size {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                for v in self.iter() {
                    // We assume that value are hold inside the heap.
                    total += Size::get_size(v);
                }

                total
            }
        }
    }
}

macro_rules! impl_size_map {
    ($name:ident) => {
        impl<K, V> Size for $name<K, V> where K: Size, V: Size {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                for (k, v) in self.iter() {
                    // We assume that keys and value are hold inside the heap.
                    total += Size::get_size(k);
                    total += Size::get_size(v);
                }

                total
            }
        }
    }
}

impl_size_map!(BTreeMap);
impl_size_set!(BTreeSet);
impl_size_set!(BinaryHeap);
impl_size_map!(HashMap);
impl_size_set!(HashSet);
impl_size_set!(LinkedList);
impl_size_set!(VecDeque);

impl_size_set!(Vec);



macro_rules! impl_size_tuple {
    ($($t:ident, $T:ident),+) => {
        impl<$($T,)*> Size for ($($T,)*)
        where
            $(
                $T: Size,
            )*
        {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                let ($($t,)*) = self;
                $(
                    total += Size::get_heap_size($t);
                )*

                total
            }
        }
    }
}

macro_rules! execute_tuple_macro_16 {
    ($name:ident) => {
        $name!(v1,V1);
        $name!(v1,V1,v2,V2);
        $name!(v1,V1,v2,V2,v3,V3);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9,v10,V10);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9,v10,V10,v11,V11);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9,v10,V10,v11,V11,v12,V12);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9,v10,V10,v11,V11,v12,V12,v13,V13);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9,v10,V10,v11,V11,v12,V12,v13,V13,v14,V14);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9,v10,V10,v11,V11,v12,V12,v13,V13,v14,V14,v15,V15);
        $name!(v1,V1,v2,V2,v3,V3,v4,V4,v5,V5,v6,V6,v7,V7,v8,V8,v9,V9,v10,V10,v11,V11,v12,V12,v13,V13,v14,V14,v15,V15,v16,V16);
    }
}

execute_tuple_macro_16!(impl_size_tuple);



impl<T, const SIZE: usize> Size for [T; SIZE] where T: Size {
    fn get_heap_size(&self) -> usize {
        let mut total = 0;

        for element in self.iter() {
            // The array stack size already accounts for the stack size of the elements of the array.
            total += Size::get_heap_size(element);
        }

        total
    }
}

/// Treats the pointed to data as being allocated on the heap.
impl<T> Size for &T where T: Size {
    fn get_heap_size(&self) -> usize {
        Size::get_size(*self)
    }
}

/// Treats the pointed to data as being allocated on the heap.
impl<T> Size for &mut T where T: Size {
    fn get_heap_size(&self) -> usize {
        Size::get_size(*self)
    }
}

impl<T> Size for Box<T> where T: Size {
    fn get_heap_size(&self) -> usize {
        Size::get_size(&**self)
    }
}

impl<T> Size for Rc<T> where T: Size {
    fn get_heap_size(&self) -> usize {
        Size::get_size(&**self)
    }
}

impl<T> Size for Arc<T> where T: Size {
    fn get_heap_size(&self) -> usize {
        Size::get_size(&**self)
    }
}

impl<T> Size for Option<T> where T: Size {
    fn get_heap_size(&self) -> usize {
        match self {
            // The options stack size already accounts for the values stack size.
            Some(t) => Size::get_heap_size(t),
            None => 0
        }
    }
}

impl<T, E> Size for Result<T, E> where T: Size, E: Size {
    fn get_heap_size(&self) -> usize {
        match self {
            // The results stack size already accounts for the values stack size.
            Ok(t) => Size::get_heap_size(t),
            Err(e) => Size::get_heap_size(e),
        }
    }
}

impl<T> Size for Mutex<T> where T: Size {
    fn get_heap_size(&self) -> usize {
        // We assume that a Mutex does hold its data at the stack.
        Size::get_heap_size(&*(self.lock().unwrap()))
    }
}

impl<T> Size for RwLock<T> where T: Size {
    fn get_heap_size(&self) -> usize {
        // We assume that a RwLock does hold its data at the stack.
        Size::get_heap_size(&*(self.read().unwrap()))
    }
}
