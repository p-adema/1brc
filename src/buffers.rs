use std::cell::UnsafeCell;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub(crate) const BLOCK_SIZE: usize = 50_000;

// Must be >1, or we might deadlock if one side drops
pub(crate) const N_BLOCKS: usize = 5;

pub(crate) struct BufferOwner(#[allow(dead_code)]Box<[UnsafeCell<[BufferBlock; N_BLOCKS]>]>);
pub(crate) type FillHandles = Vec<ThreadBufferHandle<FillSide>>;

pub(crate) type ParseHandles = Vec<ThreadBufferHandle<ParseSide>>;

/// Caller must ensure that neither `BufferOwner` nor any of the `ThreadBufferHandle` objects
/// are dropped before the last `ThreadBufferHandle::get_fillable` or `ThreadBufferHandle::get_parseable` call.
/// If any of the `ThreadBufferHandle` handles are dropped prematurely, the next call by the opposing side will panic.
/// If the `BufferOwner` is dropped, the next call will segfault.
pub(crate) unsafe fn init_buffers(parse_threads: usize) -> (BufferOwner, FillHandles, ParseHandles) {
    let owner = (0..parse_threads)
        .map(|_| {
            UnsafeCell::new([0; N_BLOCKS].map(|_| BufferBlock::new()))
        })
        .collect::<Box<_>>();
    let fill = owner.iter().map(|b| ThreadBufferHandle(b.get(), PhantomData)).collect();
    let parse = owner.iter().map(|b| ThreadBufferHandle(b.get(), PhantomData)).collect();
    (BufferOwner(owner), fill, parse)
}

pub(crate) trait BufferSide {}


pub(crate) struct FillSide {}

impl BufferSide for FillSide {}


pub(crate) struct ParseSide {}

impl BufferSide for ParseSide {}

#[repr(transparent)]
pub(crate) struct ThreadBufferHandle<S: BufferSide>(*mut [BufferBlock; N_BLOCKS], PhantomData<S>);

impl<S: BufferSide> Drop for ThreadBufferHandle<S> {
    fn drop(&mut self) {
        unsafe {
            for block in (*self.0).iter() {
                // As long as N_BLOCKS is larger than one, even if we race and lose
                // one of the Dropped markers, one of the other ones will be detected
                // and the other side will panic.
                // Unless, of course, the owning buffer is dropped. Then we segfault
                *block.state.get() = BlockState::Dropped
            }
        }
    }
}

impl ThreadBufferHandle<FillSide> {
    #[inline]
    pub(crate) fn available(&self) -> impl Iterator<Item=BlockRef<FillCleanup>> {
        unsafe { (*self.0).iter().filter_map(|b| b.try_fill()) }
    }
}

impl ThreadBufferHandle<ParseSide> {
    #[inline]
    pub(crate) fn available(&self) -> impl Iterator<Item=BlockRef<ParseCleanup>> {
        unsafe {
            (*self.0).iter().filter_map(|b| b.try_parse())
        }
    }
}

pub(crate) trait BlockCleanup: Sized {
    fn clean(block_ref: &mut BlockRef<'_, Self>);
}

pub(crate) struct FillCleanup {}

impl BlockCleanup for FillCleanup {
    #[inline]
    fn clean(block_ref: &mut BlockRef<'_, Self>) {
        debug_assert!(matches!(unsafe {&*block_ref.1}, &BlockState::Empty));
        unsafe { block_ref.1.write(BlockState::Filled) };
    }
}

pub(crate) struct ParseCleanup {}

impl BlockCleanup for ParseCleanup {
    #[inline]
    fn clean(block_ref: &mut BlockRef<'_, Self>) {
        debug_assert!(matches!(unsafe {&*block_ref.1}, &BlockState::Filled));
        unsafe { block_ref.1.write(BlockState::Empty) };
    }
}

enum BlockState {
    Empty,
    Filled,
    Dropped,
}

pub(crate) struct BufferBlock {
    state: UnsafeCell<BlockState>,
    buf: UnsafeCell<[u8; BLOCK_SIZE]>,
}

impl Debug for BufferBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        unsafe {
            match *self.state.get() {
                BlockState::Empty => { write!(f, "BufferBlock(Empty)") }
                BlockState::Filled => { write!(f, "BufferBlock(Filled)") }
                BlockState::Dropped => { write!(f, "BufferBlock(Dropped!)") }
            }
        }
    }
}

unsafe impl Send for ThreadBufferHandle<ParseSide> {}

unsafe impl Sync for ThreadBufferHandle<ParseSide> {}

impl BufferBlock {
    fn new() -> Self {
        Self {
            buf: UnsafeCell::new([0; BLOCK_SIZE]),
            state: UnsafeCell::new(BlockState::Empty)
        }
    }

    #[inline]
    pub(crate) unsafe fn try_fill(&self) -> Option<BlockRef<FillCleanup>> {
        match *self.state.get() {
            BlockState::Empty => Some(self.get_ref()),
            BlockState::Filled => None,
            BlockState::Dropped => panic!("Parser was dropped too soon!")
        }
    }
    #[inline]
    pub(crate) unsafe fn try_parse(&self) -> Option<BlockRef<ParseCleanup>> {
        match *self.state.get() {
            BlockState::Filled => Some(self.get_ref()),
            BlockState::Empty => None,
            BlockState::Dropped => panic!("Filler was dropped too soon!")
        }
    }

    #[inline]
    unsafe fn get_ref<C: BlockCleanup>(&self) -> BlockRef<C> {
        BlockRef(&mut *self.buf.get(), self.state.get(), PhantomData)
    }
}

pub(crate) struct BlockRef<'a, C: BlockCleanup>(&'a mut [u8; BLOCK_SIZE], *mut BlockState, PhantomData<C>);

impl<'a, C: BlockCleanup> Deref for BlockRef<'a, C> {
    type Target = [u8; BLOCK_SIZE];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a, C: BlockCleanup> DerefMut for BlockRef<'a, C> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a, C: BlockCleanup> Drop for BlockRef<'a, C> {
    #[inline]
    fn drop(&mut self) {
        C::clean(self)
    }
}



