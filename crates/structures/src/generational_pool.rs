use std::{
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZeroU64,
};

use self::inline_free_list::{InlineFreeList, Item, ItemRef};

/// Nonzero handle to an item in a generational pool. The handle is guaranteed
/// to be unique for the lifetime of the pool that created it.
///
/// It is typed for a modicum of safety, but it is still possible to use the
/// handle manipulate objects in a different pool of the same type, which is
/// undefined behavior!
pub struct Handle<T>(NonZeroU64, PhantomData<T>);

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Handle<T> {}

impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<T> Ord for Handle<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T> From<Handle_> for Handle<T> {
    fn from(handle: Handle_) -> Self {
        debug_assert!(handle.index != 0 || handle.generation != 0);

        let value = u64::from(handle.generation) << 32 | u64::from(handle.index);
        Self(unsafe { NonZeroU64::new_unchecked(value) }, PhantomData)
    }
}

impl<T> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Handle_ { index, generation } = (*self).into();
        f.debug_struct("Handle")
            .field("index", &index)
            .field("generation", &generation)
            .finish()
    }
}

impl<T> Handle<T> {
    /// Converts the handle into a raw index.
    ///
    /// ## Safety
    ///
    /// This is unsafe because it discards the generation component of the
    /// handle. Use this only if you know that the handle will be valid for as
    /// long as you intend to use it.
    #[must_use]
    pub unsafe fn raw(&self) -> RawHandle<T> {
        RawHandle {
            index: self.0.get() as u32,
            _phantom: PhantomData,
        }
    }
}

struct Handle_ {
    index: u32,
    generation: u32,
}

impl<T> From<Handle<T>> for Handle_ {
    fn from(handle: Handle<T>) -> Self {
        let raw = handle.0.get();
        Self {
            index: raw as u32,
            generation: (raw >> 32) as u32,
        }
    }
}

/// An unchecked handle to a slot in a [`GenerationalPool`].
///
/// This is a raw index into the pool's internal storage. It is not guaranteed
/// to be valid or unique. Use this only if you know that the slot will not be
/// freed before you intend to use it.
///
/// Note that strictly speaking, accessing a slot through a raw handle is safe,
/// since the pool will still check for liveness. However, it is still possible
/// that the slot was freed and replaced by a different value.
pub struct RawHandle<T> {
    index: u32,
    _phantom: PhantomData<T>,
}

impl<T> Clone for RawHandle<T> {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            _phantom: PhantomData,
        }
    }
}

impl<T> Copy for RawHandle<T> {}

impl<T> PartialEq for RawHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T> Eq for RawHandle<T> {}

impl<T> From<Handle_> for RawHandle<T> {
    fn from(handle: Handle_) -> Self {
        Self {
            index: handle.index,
            _phantom: PhantomData,
        }
    }
}

impl<T> std::fmt::Debug for RawHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawHandle")
            .field("index", &self.index)
            .finish()
    }
}

impl<T> std::hash::Hash for RawHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

struct Slot<T> {
    // This is kept outside of the item so that it won't be overwritten when
    // the slot is added to the free list.
    generation: u32,
    item: Item<T>,
}

impl<T> ItemRef<T> for Slot<T> {
    fn item_mut(&mut self) -> &mut Item<T> {
        &mut self.item
    }
}

/// An object pool that makes use of generational indices to avoid the ABA
/// problem.
pub struct GenerationalPool<T> {
    items: Vec<Slot<T>>,
    free_list: InlineFreeList<T>,
}

impl<T> Default for GenerationalPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> GenerationalPool<T> {
    /// Initializes a new empty pool.
    #[must_use]
    pub fn new() -> Self {
        let mut items = vec![Slot {
            generation: 1,
            item: Item::default(),
        }];

        let mut free_list = InlineFreeList::new();
        free_list.push(0, &mut items);

        Self { items, free_list }
    }

    /// Returns a reference to the item identified by the given handle.
    ///
    /// ## Returns
    ///
    /// `Some(&T)` if the handle is valid and `None` otherwise.
    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        let handle = Handle_::from(handle);
        if self.free_list.is_free(handle.index) {
            None
        } else {
            let slot = self.items.get(handle.index as usize)?;
            (slot.generation == handle.generation)
                .then(|| unsafe { slot.item.value.assume_init_ref() })
        }
    }

    /// Returns a mutable reference to the item identified by the given handle.
    ///
    /// ## Returns
    ///
    /// `Some(&mut T)` if the handle is valid and `None` otherwise.
    #[must_use]
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let handle = Handle_::from(handle);
        if self.free_list.is_free(handle.index) {
            None
        } else {
            let slot = self.items.get_mut(handle.index as usize)?;
            (slot.generation == handle.generation)
                .then(|| unsafe { (*slot.item.value).assume_init_mut() })
        }
    }

    /// Returns a reference to the item identified by the given handle without
    /// checking its generation.
    ///
    /// ## Safety
    ///
    /// This function is safe, in that it will not cause undefined behavior.
    /// However, it is possible for a slot to be freed and replaced by a
    /// different value.
    ///
    /// ## Panics
    ///
    /// This function will panic if the handle does not refer to a slot with
    /// valid data.
    #[must_use]
    pub fn get_raw(&self, handle: RawHandle<T>) -> &T {
        self.raw_(handle).1
    }

    /// Returns a mutable reference to the item identified by the given handle
    /// without checking its generation.
    ///
    /// ## Safety
    ///
    /// This function is safe, in that it will not cause undefined behavior.
    /// However, it is possible for a slot to be freed and replaced by a
    /// different value.
    ///
    /// ## Panics
    ///
    /// This function will panic if the handle does not refer to a slot with
    /// valid data.
    #[must_use]
    pub fn get_raw_mut(&mut self, handle: RawHandle<T>) -> &mut T {
        self.raw_mut_(handle).1
    }

    /// Validates a handle and returns a raw handle if it is valid. This is the
    /// safe way to obtain a [`RawHandle<T>`].
    #[must_use]
    pub fn validate(&self, handle: Handle<T>) -> Option<RawHandle<T>> {
        let handle = Handle_::from(handle);

        if self.free_list.is_free(handle.index) {
            None
        } else {
            let slot = self.items.get(handle.index as usize)?;
            (slot.generation == handle.generation).then_some(handle.into())
        }
    }

    /// Attempts to recover a handle from a raw handle.
    #[inline]
    #[must_use]
    pub fn recover_key(&self, handle: RawHandle<T>) -> Option<Handle<T>> {
        if self.free_list.is_free(handle.index) {
            None
        } else {
            // SAFETY: This is safe because we know that the slot is not free.
            let slot = unsafe { self.items.get_unchecked(handle.index as usize) };
            Some(Self::handle_(handle.index, slot.generation))
        }
    }

    /// Inserts a new value into the pool and returns a handle to it.
    ///
    /// ## Panics
    ///
    /// This function will panic if the pool would exceed `isize::MAX` bytes in
    /// size.
    #[must_use]
    pub fn insert(&mut self, value: T) -> Handle<T> {
        if let Some((index, item)) = self.free_list.pop(&mut self.items) {
            let handle = Self::handle_(index, item.generation);
            item.item.value = ManuallyDrop::new(MaybeUninit::new(value));
            handle
        } else {
            let index = u32::try_from(self.items.len()).expect("max u32::MAX items!");
            let handle = Self::handle_(index, 0);

            self.items.push(Slot {
                generation: 0,
                item: Item {
                    value: ManuallyDrop::new(MaybeUninit::new(value)),
                },
            });

            handle
        }
    }

    /// Removes the value identified by the given handle from the pool.
    ///
    /// ## Returns
    ///
    /// Returns the value if the handle is valid and `None` otherwise.
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let raw_handle = self.validate(handle)?;
        let handle = Handle_::from(handle);

        // SAFETY: Asserting that the slot contains valid data implies that the
        // index is within bounds of the array.
        let slot = unsafe { self.items.get_unchecked_mut(raw_handle.index as usize) };

        if slot.generation == handle.generation {
            // SAFETY: We have checked that the slot contains valid data implies
            // that the value is initialized.
            let value = unsafe { ManuallyDrop::take(&mut slot.item.value).assume_init() };

            // If the slot is not saturated, we can reuse it.
            if slot.generation < u32::MAX {
                slot.generation += 1;
                self.free_list.push(handle.index, &mut self.items);
            }

            Some(value)
        } else {
            None
        }
    }

    /// Removes the value identified by the given handle from the pool without
    /// checking its generation.
    ///
    /// ## Panics
    ///
    /// This function will panic if the handle points to a free slot.
    pub fn remove_raw(&mut self, handle: RawHandle<T>) -> T {
        // SAFETY: This checks that the handle is not free (refers to valid
        // data).
        assert!(
            !self.free_list.is_free(handle.index),
            "raw handle refers to free slot"
        );

        // SAFETY: Asserting that the slot contains valid data implies that the
        // index is within bounds of the array.
        let slot = unsafe { self.items.get_unchecked_mut(handle.index as usize) };

        // SAFETY: Asserting that the slot contains valid data implies that the
        // value is initialized.
        let value = unsafe { ManuallyDrop::take(&mut slot.item.value).assume_init() };

        // If the slot is not saturated, we can reuse it.
        if slot.generation < u32::MAX {
            slot.generation += 1;
            self.free_list.push(handle.index, &mut self.items);
        }

        value
    }

    #[inline]
    fn raw_(&self, handle: RawHandle<T>) -> (u32, &T) {
        // SAFETY: This checks that the handle is not free (refers to valid
        // data).
        assert!(
            !self.free_list.is_free(handle.index),
            "raw handle refers to free slot"
        );

        // SAFETY: Asserting that the slot contains valid data implies that the
        // index is within bounds of the array.
        unsafe {
            let slot = self.items.get_unchecked(handle.index as usize);
            (slot.generation, slot.item.value.assume_init_ref())
        }
    }

    #[inline]
    fn raw_mut_(&mut self, handle: RawHandle<T>) -> (&mut u32, &mut T) {
        // SAFETY: This checks that the handle is not free (refers to valid
        // data).
        assert!(
            !self.free_list.is_free(handle.index),
            "raw handle refers to free slot"
        );

        // SAFETY: Asserting that the slot contains valid data implies that the
        // index is within bounds of the array.
        unsafe {
            let slot = self.items.get_unchecked_mut(handle.index as usize);
            (&mut slot.generation, (*slot.item.value).assume_init_mut())
        }
    }

    #[inline]
    fn handle_(index: u32, generation: u32) -> Handle<T> {
        Handle_ { index, generation }.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init() {
        let gen = GenerationalPool::<u32>::new();

        assert_eq!(gen.items.len(), 1);
        assert_eq!(gen.items[0].generation, 1);
        assert!(!gen.free_list.is_empty());
        assert_eq!(gen.free_list.next, Some(0));
    }

    #[test]
    fn insert_get_remove_one() {
        let mut gen = GenerationalPool::<u32>::new();

        let handle = gen.insert(42);

        assert_eq!(
            handle,
            Handle_ {
                index: 0,
                generation: 1
            }
            .into()
        );

        assert_eq!(gen.get(handle), Some(&42));
        assert_eq!(gen.get_mut(handle), Some(&mut 42));
        assert!(gen.free_list.is_empty());

        assert_eq!(gen.remove(handle), Some(42));
        assert_eq!(gen.validate(handle), None);
        assert!(!gen.free_list.is_empty());
        assert_eq!(gen.free_list.next, Some(0));
        assert_eq!(gen.items[0].generation, 2);
    }

    #[test]
    fn handles() {
        let mut gen = GenerationalPool::<u32>::new();

        let handle = gen.insert(42);
        let handle2 = handle.clone();

        assert_eq!(handle, handle2);

        assert_eq!(
            format!("{:?}", handle),
            "Handle { index: 0, generation: 1 }"
        );

        assert_eq!(
            format!("{:?}", unsafe { handle.raw() }),
            "RawHandle { index: 0 }"
        );
    }

    #[test]
    fn insert_get_remove_many() {
        const COUNT: usize = 10;

        let mut gen = GenerationalPool::<u32>::new();
        let mut keys = vec![];

        for i in 0..COUNT {
            let handle = gen.insert(i as u32);
            keys.push(handle);
        }

        for (i, k) in keys.iter().enumerate() {
            assert_eq!(gen.get(*k), Some(&(i as u32)));
            assert_eq!(gen.get_mut(*k), Some(&mut (i as u32)));
        }

        assert_eq!(gen.items.len(), COUNT);

        for (i, k) in keys.iter().enumerate() {
            assert_eq!(gen.remove(*k), Some(i as u32));
            assert_eq!(gen.validate(*k), None);
        }

        assert_eq!(gen.items.len(), COUNT);

        let mut i = gen.free_list.next;
        while let Some(index) = i {
            if index == 0 {
                assert_eq!(gen.items[index as usize].generation, 2);
            } else {
                assert_eq!(gen.items[index as usize].generation, 1);
            }

            assert!(gen.free_list.is_free(index));
            i = unsafe { gen.items[index as usize].item.next };
        }
    }

    #[test]
    fn remove_twice() {
        let mut gen = GenerationalPool::<u32>::new();
        let handle = gen.insert(42);

        assert_eq!(gen.remove(handle), Some(42));
        assert_eq!(gen.remove(handle), None);

        let _ = gen.insert(43);
        assert_eq!(gen.remove(handle), None);
    }

    #[test]
    fn insert_remove_insert_get() {
        let mut gen = GenerationalPool::<u32>::new();

        let a = gen.insert(42);
        assert_eq!(gen.remove(a), Some(42));

        let b = gen.insert(43);
        assert_eq!(gen.get(a), None);
        assert_eq!(gen.get(b), Some(&43));

        unsafe { assert_eq!(a.raw(), b.raw()) };
    }

    #[test]
    fn non_copy_drop_once() {
        static mut DROP_COUNT: usize = 0;

        struct T {
            #[allow(dead_code)]
            data: u32,
        }

        impl Drop for T {
            fn drop(&mut self) {
                unsafe { DROP_COUNT += 1 };
            }
        }

        let mut gen = GenerationalPool::<T>::new();

        {
            let a = gen.insert(T { data: 42 });
            gen.remove(a);

            assert_eq!(unsafe { DROP_COUNT }, 1);
        }

        {
            let a = gen.insert(T { data: 42 });
            let a = gen.validate(a).unwrap();

            gen.remove_raw(a);
            assert_eq!(unsafe { DROP_COUNT }, 2);
        }
    }
}

mod inline_free_list {
    use std::mem::{ManuallyDrop, MaybeUninit};

    use crate::flagvec::FlagVec;

    /// A place to store the next free item in the free list. A value that is
    /// undefined when the item is free can be stored in the same memory for
    /// compactness.
    #[derive(Clone, Copy)]
    pub union Item<T> {
        pub next: Option<u32>,
        pub value: ManuallyDrop<MaybeUninit<T>>,
    }

    impl<T> Default for Item<T> {
        fn default() -> Self {
            Self { next: None }
        }
    }

    /// A trait to allow the free list next pointer to be extracted from a
    /// larger struct.
    pub trait ItemRef<T> {
        fn item_mut(&mut self) -> &mut Item<T>;
    }

    impl<T> ItemRef<T> for Item<T> {
        fn item_mut(&mut self) -> &mut Item<T> {
            self
        }
    }

    pub struct InlineFreeList<T> {
        pub next: Option<u32>,
        is_free: FlagVec,
        phantom: std::marker::PhantomData<T>,
    }

    impl<T> InlineFreeList<T> {
        pub fn new() -> Self {
            Self {
                next: None,
                is_free: FlagVec::new(),
                phantom: std::marker::PhantomData,
            }
        }

        pub fn is_free(&self, index: u32) -> bool {
            self.is_free.get(index as usize)
        }

        #[cfg(test)]
        pub fn is_empty(&self) -> bool {
            self.next.is_none()
        }

        pub fn push(&mut self, index: u32, items: &mut [impl ItemRef<T>]) {
            assert!(!self.is_free(index));

            let item = items[index as usize].item_mut();
            item.next = self.next;
            self.next = Some(index);

            self.is_free.set(index as usize, true);
        }

        pub fn pop<'a, K: ItemRef<T>>(&mut self, items: &'a mut [K]) -> Option<(u32, &'a mut K)> {
            let index = self.next?;
            assert!(self.is_free(index));

            let item = &mut items[index as usize];

            // SAFETY: The item is free (checked in the above assertion), so it
            // must have a next pointer.
            self.next = unsafe { item.item_mut().next };

            self.is_free.set(index as usize, false);

            // SAFETY: No further uses of item.next are permitted until it is
            // pushed on to the free list again.
            Some((index, item))
        }
    }
}
