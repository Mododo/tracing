// taken from https://github.com/hyperium/http/blob/master/src/extensions.rs.

use crate::sync::{RwLockReadGuard, RwLockWriteGuard};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    hash::{BuildHasherDefault, Hasher},
};

#[allow(warnings)]
type AnyMap = HashMap<TypeId, Box<dyn Any + Send + Sync>, BuildHasherDefault<IdHasher>>;

// With TypeIds as keys, there's no need to hash them. They are already hashes
// themselves, coming from the compiler. The IdHasher just holds the u64 of
// the TypeId, and then returns it, instead of doing any bit fiddling.
#[derive(Default)]
pub struct IdHasher(u64);

impl Hasher for IdHasher {
    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

/// An immutable, read-only reference to a Span's extensions.
#[derive(Debug)]
pub struct Extensions<'a> {
    inner: RwLockReadGuard<'a, ExtensionsInner>,
}

impl<'a> Extensions<'a> {
    pub(crate) fn new(inner: RwLockReadGuard<'a, ExtensionsInner>) -> Self {
        Self { inner }
    }

    /// Get a mutable reference to a type previously inserted on this `Extensions`.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.inner.get::<T>()
    }
}

/// An mutable reference to a Span's extensions.
#[derive(Debug)]
pub struct ExtensionsMut<'a> {
    inner: RwLockWriteGuard<'a, ExtensionsInner>,
}

impl<'a> ExtensionsMut<'a> {
    pub(crate) fn new(inner: RwLockWriteGuard<'a, ExtensionsInner>) -> Self {
        Self { inner }
    }

    /// Insert a type into this `Extensions`.
    ///
    /// If a extension of this type already existed, it will
    /// be returned.
    pub fn insert<T: Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
        self.inner.insert(val)
    }

    /// Get a mutable reference to a type previously inserted on this `ExtensionsMut`.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.inner.get_mut::<T>()
    }
}

/// A type map of span extensions.
///
/// [ExtensionsInner] is used by [Data] to store and
/// span-specific data. A given [Layer] can read and write
/// data that it is interested in recording and emitting.
#[derive(Default)]
pub(crate) struct ExtensionsInner {
    // If extensions are never used, no need to carry around an empty HashMap.
    // That's 3 words. Instead, this is only 1 word.
    map: Option<Box<AnyMap>>,
}

impl ExtensionsInner {
    /// Create an empty `Extensions`.
    #[inline]
    pub(crate) fn new() -> ExtensionsInner {
        ExtensionsInner { map: None }
    }

    /// Insert a type into this `Extensions`.
    ///
    /// If a extension of this type already existed, it will
    /// be returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_subscriber::registry::::ExtensionsInner;
    /// let mut ext = ExtensionsInner::new();
    /// assert!(ext.insert(5i32).is_none());
    /// assert!(ext.insert(4u8).is_none());
    /// assert_eq!(ext.insert(9i32), Some(5i32));
    /// ```
    pub(crate) fn insert<T: Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .get_or_insert_with(|| Box::new(HashMap::default()))
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(|boxed| {
                #[allow(warnings)]
                {
                    (boxed as Box<Any + 'static>)
                        .downcast()
                        .ok()
                        .map(|boxed| *boxed)
                }
            })
    }

    /// Get a reference to a type previously inserted on this `Extensions`.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_subscriber::registry::::Extensions;
    /// let mut ext = Extensions::new();
    /// assert!(ext.get::<i32>().is_none());
    /// ext.insert(5i32);
    ///
    /// assert_eq!(ext.get::<i32>(), Some(&5i32));
    /// ```
    pub(crate) fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .as_ref()
            .and_then(|map| map.get(&TypeId::of::<T>()))
            .and_then(|boxed| (&**boxed as &(dyn Any + 'static)).downcast_ref())
    }

    /// Get a mutable reference to a type previously inserted on this `Extensions`.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_subscriber::registry::::Extensions;
    /// let mut ext = Extensions::new();
    /// ext.insert(String::from("Hello"));
    /// ext.get_mut::<String>().unwrap().push_str(" World");
    ///
    /// assert_eq!(ext.get::<String>().unwrap(), "Hello World");
    /// ```
    pub(crate) fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .as_mut()
            .and_then(|map| map.get_mut(&TypeId::of::<T>()))
            .and_then(|boxed| (&mut **boxed as &mut (dyn Any + 'static)).downcast_mut())
    }

    /// Remove a type from this `Extensions`.
    ///
    /// If a extension of this type existed, it will be returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::Extensions;
    /// let mut ext = Extensions::new();
    /// ext.insert(5i32);
    /// assert_eq!(ext.remove::<i32>(), Some(5i32));
    /// assert!(ext.get::<i32>().is_none());
    /// ```
    pub(crate) fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.map
            .as_mut()
            .and_then(|map| map.remove(&TypeId::of::<T>()))
            .and_then(|boxed| {
                #[allow(warnings)]
                {
                    (boxed as Box<Any + 'static>)
                        .downcast()
                        .ok()
                        .map(|boxed| *boxed)
                }
            })
    }

    /// Clear the `Extensions` of all inserted extensions.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_subscriber::registry::::Extensions;
    /// let mut ext = Extensions::new();
    /// ext.insert(5i32);
    /// ext.clear();
    ///
    /// assert!(ext.get::<i32>().is_none());
    /// ```
    #[inline]
    pub(crate) fn clear(&mut self) {
        if let Some(ref mut map) = self.map {
            map.clear();
        }
    }
}

impl fmt::Debug for ExtensionsInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extensions").finish()
    }
}

#[test]
fn test_extensions() {
    #[derive(Debug, PartialEq)]
    struct MyType(i32);

    let mut extensions = ExtensionsInner::new();

    extensions.insert(5i32);
    extensions.insert(MyType(10));

    assert_eq!(extensions.get(), Some(&5i32));
    assert_eq!(extensions.get_mut(), Some(&mut 5i32));

    assert_eq!(extensions.remove::<i32>(), Some(5i32));
    assert!(extensions.get::<i32>().is_none());

    assert_eq!(extensions.get::<bool>(), None);
    assert_eq!(extensions.get(), Some(&MyType(10)));
}
