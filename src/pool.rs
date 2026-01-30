use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fmt::{self, Debug};
use std::sync::Arc;

/// A thread-safe object pool that pre-allocates and reuses objects to avoid runtime allocations
/// on hot paths.
pub struct ObjectPool<T> {
    /// The collection of available objects
    available: Mutex<VecDeque<T>>,

    /// Factory function for creating new objects when the pool is empty
    create_fn: Arc<dyn Fn() -> T + Send + Sync>,

    /// Maximum size of the pool
    max_size: usize,
}

impl<T: std::fmt::Debug> std::fmt::Debug for ObjectPool<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectPool")
            .field("available", &self.available)
            .field("max_size", &self.max_size)
            .field("create_fn", &"<function>")
            .finish()
    }
}

// Base implementation for all ObjectPools regardless of T
impl<T> ObjectPool<T> {
    /// Return an object to the pool.
    ///
    /// If the pool is full, the object will be dropped.
    fn return_object(&self, object: T) {
        let mut available = self.available.lock();
        if available.len() < self.max_size {
            available.push_back(object);
        }
        // If the pool is full, the object will be dropped
    }
}

// Additional implementation for types that are Send + 'static
impl<T: Send + 'static> ObjectPool<T> {
    /// Create a new object pool with a specified capacity and factory function.
    ///
    /// # Arguments
    ///
    /// * `initial_size` - Number of objects to pre-allocate
    /// * `max_size` - Maximum number of objects to keep in the pool
    /// * `create_fn` - Function to create new objects when needed
    ///
    /// # Returns
    ///
    /// A new `ObjectPool<T>` pre-filled with `initial_size` objects
    #[must_use]
    pub fn new<F>(initial_size: usize, max_size: usize, create_fn: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        let create_fn = Arc::new(create_fn);
        let factory = Arc::clone(&create_fn);

        // Pre-allocate objects
        let mut available = VecDeque::with_capacity(max_size);
        for _ in 0..initial_size {
            available.push_back((factory)());
        }

        Self {
            available: Mutex::new(available),
            create_fn,
            max_size,
        }
    }

    /// Get an object from the pool or create a new one if the pool is empty.
    ///
    /// # Returns
    ///
    /// A `PooledObject<T>` that will return to the pool when dropped
    /// Get an object from the pool. If the pool is empty, creates a new object.
    pub fn get(&self) -> PooledObject<'_, T> {
        let object = {
            let mut available = self.available.lock();
            available.pop_front().unwrap_or_else(|| (self.create_fn)())
        };

        PooledObject {
            object: Some(object),
            pool: self,
        }
    }
}

/// A smart pointer for objects borrowed from an `ObjectPool`.
///
/// When dropped, the object is returned to the pool if the pool isn't full.
pub struct PooledObject<'a, T> {
    /// The object borrowed from the pool (None if already returned)
    object: Option<T>,

    /// Reference to the pool this object belongs to
    pool: &'a ObjectPool<T>,
}

impl<T> PooledObject<'_, T> {
    /// Consume the pooled object and return it to the pool early.
    /// This is useful when you're done with the object but it's not going out of scope yet.
    pub fn return_to_pool(mut self) {
        if let Some(object) = self.object.take() {
            self.pool.return_object(object);
        }
    }
}

impl<T> Drop for PooledObject<'_, T> {
    fn drop(&mut self) {
        if let Some(object) = self.object.take() {
            self.pool.return_object(object);
        }
    }
}

impl<T> std::ops::Deref for PooledObject<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.object
            .as_ref()
            .expect("Object already returned to pool")
    }
}

impl<T> std::ops::DerefMut for PooledObject<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.object
            .as_mut()
            .expect("Object already returned to pool")
    }
}

impl<T: Debug> Debug for PooledObject<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PooledObject")
            .field("object", &self.object)
            .finish()
    }
}

/// A pool for reusing String objects to avoid allocations in hot paths.
#[derive(Debug)]
pub struct StringPool {
    /// The internal object pool for strings
    inner: ObjectPool<String>,
}

impl StringPool {
    /// Create a new string pool with default settings.
    #[must_use]
    pub fn new(initial_size: usize, max_size: usize, initial_capacity: usize) -> Self {
        Self {
            inner: ObjectPool::new(initial_size, max_size, move || {
                String::with_capacity(initial_capacity)
            }),
        }
    }

    /// Get a string from the pool or create a new one if the pool is empty.
    pub fn get(&self) -> PooledString<'_> {
        let mut string = self.inner.get();
        string.clear(); // Ensure the string is empty
        PooledString(string)
    }

    /// Get a string from the pool and initialize it with the provided value.
    pub fn get_with_value<S: AsRef<str>>(&self, value: S) -> PooledString<'_> {
        let mut string = self.inner.get();
        string.clear(); // Ensure the string is empty
        string.push_str(value.as_ref());
        PooledString(string)
    }
}

/// A smart pointer for strings borrowed from a `StringPool`.
pub struct PooledString<'a>(PooledObject<'a, String>);

impl PooledString<'_> {
    /// Consume the pooled string and return it to the pool early.
    pub fn return_to_pool(self) {
        self.0.return_to_pool();
    }
}

impl std::ops::Deref for PooledString<'_> {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for PooledString<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Debug for PooledString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&**self, f)
    }
}

/// Implement Display for `PooledString` to allow it to be used in string formatting and logs
impl fmt::Display for PooledString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

/// Pool of reusable `Vec<T>` objects to avoid allocations on hot paths.
pub struct VecPool<T> {
    /// Inner object pool
    inner: ObjectPool<Vec<T>>,
}

impl<T: Send + 'static> VecPool<T> {
    /// Create a new vector pool with default settings.
    #[must_use]
    pub fn new(initial_size: usize, max_size: usize, initial_capacity: usize) -> Self {
        Self {
            inner: ObjectPool::new(initial_size, max_size, move || {
                Vec::with_capacity(initial_capacity)
            }),
        }
    }

    /// Get a vector from the pool.
    pub fn get(&self) -> PooledVec<'_, T> {
        let mut vec = self.inner.get();
        vec.clear(); // Ensure it's empty
        PooledVec(vec)
    }
}

/// A smart pointer for vectors borrowed from a `VecPool`.
pub struct PooledVec<'a, T>(PooledObject<'a, Vec<T>>);

impl<T> PooledVec<'_, T> {
    /// Consume the pooled vector and return it to the pool early.
    pub fn return_to_pool(self) {
        self.0.return_to_pool();
    }
}

impl<T> std::ops::Deref for PooledVec<'_, T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for PooledVec<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Debug> Debug for PooledVec<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&**self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_pool() {
        let pool = ObjectPool::new(5, 10, || String::from("test"));

        // Get an object from the pool
        let mut obj1 = pool.get();
        assert_eq!(*obj1, "test");

        // Modify the object
        obj1.push_str("-modified");
        assert_eq!(*obj1, "test-modified");

        // Return the object to the pool
        drop(obj1);

        // Get another object from the pool
        // The implementation creates a new object with the factory function
        // when popping from the pool, so it will be "test" again
        let mut obj2 = pool.get();
        assert_eq!(*obj2, "test");

        // We can modify this object independently
        obj2.push_str("-new");
        assert_eq!(*obj2, "test-new");
    }

    #[test]
    fn test_string_pool() {
        let pool = StringPool::new(5, 10, 32);

        // Get a string from the pool
        let mut str1 = pool.get();
        str1.push_str("hello");
        assert_eq!(*str1, "hello");

        // Return the string to the pool
        drop(str1);

        // Get another string from the pool (should be empty)
        let str2 = pool.get();
        assert_eq!(*str2, "");

        // Get a pre-filled string
        let str3 = pool.get_with_value("world");
        assert_eq!(*str3, "world");
    }

    #[test]
    fn test_vec_pool() {
        let pool = VecPool::new(5, 10, 32);

        // Get a vec from the pool
        let mut vec1 = pool.get();
        vec1.push(1);
        vec1.push(2);
        assert_eq!(*vec1, vec![1, 2]);

        // Return the vec to the pool
        drop(vec1);

        // Get another vec from the pool (should be empty)
        let vec2 = pool.get();
        assert_eq!(*vec2, Vec::<i32>::new());
    }
}
