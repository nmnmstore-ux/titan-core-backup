use crossbeam::queue::SegQueue;
use once_cell::sync::Lazy;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct ObjectPool<T: Default> {
    free: SegQueue<Box<T>>,
    allocation_count: AtomicU64,
    max_size: usize,
    _phantom: PhantomData<T>,
}

impl<T: Default> ObjectPool<T> {
    pub fn new(max_size: usize) -> Self {
        let free = SegQueue::new();
        let prealloc = (max_size / 10).min(10000);
        for _ in 0..prealloc {
            free.push(Box::new(T::default()));
        }
        Self {
            free,
            allocation_count: AtomicU64::new(0),
            max_size,
            _phantom: PhantomData,
        }
    }

    pub fn acquire(&self) -> Pooled<T> {
        match self.free.pop() {
            Some(boxed) => Pooled {
                inner: Some(boxed),
                pool: self as *const ObjectPool<T>,
            },
            None => {
                self.allocation_count.fetch_add(1, Ordering::Relaxed);
                Pooled {
                    inner: Some(Box::new(T::default())),
                    pool: self as *const ObjectPool<T>,
                }
            }
        }
    }

    pub fn release(&self, item: Box<T>) {
        if self.free.len() < self.max_size {
            self.free.push(item);
        }
    }

    pub fn allocation_count(&self) -> u64 {
        self.allocation_count.load(Ordering::Relaxed)
    }

    pub fn available(&self) -> usize {
        self.free.len()
    }
}

pub struct Pooled<T: Default> {
    inner: Option<Box<T>>,
    pool: *const ObjectPool<T>,
}

impl<T: Default> Drop for Pooled<T> {
    fn drop(&mut self) {
        if let Some(item) = self.inner.take() {
            unsafe {
                (*self.pool).release(item);
            }
        }
    }
}

impl<T: Default> Deref for Pooled<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner.as_ref().unwrap()
    }
}

impl<T: Default> DerefMut for Pooled<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.as_mut().unwrap()
    }
}

pub static ORDER_POOL: once_cell::sync::Lazy<ObjectPool<super::types::Order>> =
    once_cell::sync::Lazy::new(|| ObjectPool::new(1_000_000));

pub fn acquire_order() -> Pooled<super::types::Order> {
    ORDER_POOL.acquire()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Debug, PartialEq)]
    struct TestObject {
        value: u64,
        label: String,
    }

    #[test]
    fn test_acquire_release() {
        let pool = ObjectPool::<TestObject>::new(100);
        let mut obj = pool.acquire();
        obj.value = 42;
        obj.label = "hello".to_string();
        assert_eq!(obj.value, 42);
        assert_eq!(obj.label, "hello");
        drop(obj);
        assert_eq!(pool.allocation_count(), 0);
    }

    #[test]
    fn test_pool_reuses_objects() {
        let pool = ObjectPool::<TestObject>::new(100);
        // prealloc = 100/10 = 10 objects. Acquire 3, release them,
        // then acquire 10 more — all from the free list, so allocation_count stays 0.
        let addr1 = {
            let obj = pool.acquire();
            &*obj as *const TestObject
        };
        let addr2 = {
            let obj = pool.acquire();
            &*obj as *const TestObject
        };
        let addr3 = {
            let obj = pool.acquire();
            &*obj as *const TestObject
        };
        assert_eq!(pool.allocation_count(), 0, "no new allocations — all from prealloc");
        assert!(addr1 != addr2 && addr2 != addr3 && addr1 != addr3, "three distinct objects acquired");
        // Release all three — they go to the back of the FIFO queue.
        // Now acquire 10 more objects. All must come from the free list
        // (preallocated + released), so allocation_count stays 0.
        let mut addrs = Vec::new();
        for _ in 0..10 {
            let obj = pool.acquire();
            addrs.push(&*obj as *const TestObject);
        }
        assert_eq!(pool.allocation_count(), 0, "still no new allocations after release");
        // addr1 was the first preallocated object. After 3 releases + 7 more pops
        // from the original prealloc queue, addr1 resurfaces as the 8th pop.
        // Verify it appears in the acquired set.
        assert!(
            addrs.iter().any(|&a| a == addr1),
            "addr1 was reused after release"
        );
    }

    #[test]
    fn test_allocation_count_increases_when_pool_exhausted() {
        let pool = ObjectPool::<TestObject>::new(10);
        let mut objects = Vec::new();
        for _ in 0..15 {
            objects.push(pool.acquire());
        }
        // prealloc = max_size / 10 = 1, then 14 more are newly allocated
        assert_eq!(pool.allocation_count(), 14);
        drop(objects);
    }

    #[test]
    fn test_pool_max_size_respected() {
        let pool = ObjectPool::<TestObject>::new(100);
        let mut objects = Vec::new();
        for _ in 0..200 {
            objects.push(pool.acquire());
        }
        let overflow_count = pool.allocation_count();
        drop(objects);
        assert!(pool.available() <= 100, "pool should not exceed max_size");
        assert!(overflow_count > 0, "should have overflow allocations");
    }

    #[test]
    fn test_concurrent_usage() {
        let pool = std::sync::Arc::new(ObjectPool::<TestObject>::new(1000));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let p = pool.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    let mut obj = p.acquire();
                    obj.value = 42;
                    drop(obj);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(pool.allocation_count() < 800);
    }

    #[test]
    fn test_deref_mut_modification() {
        let pool = ObjectPool::<TestObject>::new(10);
        let mut obj = pool.acquire();
        *obj = TestObject {
            value: 99,
            label: "modified".to_string(),
        };
        assert_eq!(obj.value, 99);
    }

    #[test]
    fn test_order_pool_integration() {
        use crate::types::Order;
        let pool = ObjectPool::<Order>::new(1000);
        let mut order = pool.acquire();
        order.price = 100.50;
        order.quantity = 10.0;
        drop(order);
        assert!(pool.allocation_count() <= 100);
    }
}
