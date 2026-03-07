use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use bytes::Buf;
use core::cmp::{max, min};
use core::iter;
use core::marker::PhantomData;
use core::mem;

#[cfg(feature = "forbid-unsafe")]
use bytes::BufMut;
#[cfg(not(feature = "forbid-unsafe"))]
use core::mem::{transmute, MaybeUninit};
#[cfg(not(feature = "forbid-unsafe"))]
use core::ptr;

// Flag for platform-specific optimization that avoids large slowdowns on some architectures.
const ENABLE_SELF_COPY_OPTIMIZATION: bool = cfg!(any(
    all(
        feature = "auto-self-copy-optimization",
        not(feature = "prefer-no-self-copy-optimization"),
        target_arch = "x86_64"
    ),
    feature = "self-copy-optimization"
));
// Prepends larger than this size will always delegate to standard ptr data copying.
const MAX_SELF_COPY: usize = 9;
// Default first allocation size.
const MIN_CHUNK_SIZE: usize = 2 * mem::size_of::<&[u8]>();

/// A prepend-only byte buffer.
///
/// It is not guaranteed to be efficient to interleave reads via `bytes::Buf` and writes via
/// `ReverseBuf::prepend`.
pub trait ReverseBuf: Buf {
    /// Prepends bytes to the buffer. These bytes will still be in the order they appear in the
    /// provided `Buf` when they are read back, but they will appear immediately before any bytes
    /// already written to the buffer.
    fn prepend<B: Buf>(&mut self, data: B);

    /// Returns the number of bytes available for additional data to be prepended. The returned
    /// value should not change if no data is prepended, should be zero when and only when no more
    /// writes can succeed, and may be smaller than the real capacity possible.
    ///
    /// This method follows the same general contract as `bytes::BufMut::remaining_mut()`.
    fn remaining_writable(&self) -> usize;

    // --- Provided: ---

    /// Corresponding prepending method to `bytes::BufMut::put_slice`.
    #[inline]
    fn prepend_slice(&mut self, data: &[u8]) {
        self.prepend(data)
    }

    /// Prepends the bytes of an array given by-value to the buffer.
    #[inline]
    fn prepend_array<const N: usize>(&mut self, data: [u8; N]) {
        self.prepend_slice(&data)
    }

    /// Corresponding prepending method to `bytes::BufMut::put_u8`.
    #[inline]
    fn prepend_u8(&mut self, n: u8) {
        self.prepend_array([n]);
    }

    /// Corresponding prepending method to `bytes::BufMut::put_i8`.
    #[inline]
    fn prepend_i8(&mut self, n: i8) {
        self.prepend_u8(n as u8);
    }

    /// Corresponding prepending method to `bytes::BufMut::put_u16_le`.
    #[inline]
    fn prepend_u16_le(&mut self, n: u16) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_i16_le`.
    #[inline]
    fn prepend_i16_le(&mut self, n: i16) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_u32_le`.
    #[inline]
    fn prepend_u32_le(&mut self, n: u32) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_i32_le`.
    #[inline]
    fn prepend_i32_le(&mut self, n: i32) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_u64_le`.
    #[inline]
    fn prepend_u64_le(&mut self, n: u64) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_i64_le`.
    #[inline]
    fn prepend_i64_le(&mut self, n: i64) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_f32_le`.
    #[inline]
    fn prepend_f32_le(&mut self, n: f32) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_f64_le`.
    #[inline]
    fn prepend_f64_le(&mut self, n: f64) {
        self.prepend_array(n.to_le_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_u16_be`.
    #[inline]
    fn prepend_u16_be(&mut self, n: u16) {
        self.prepend_array(n.to_be_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_i16_be`.
    #[inline]
    fn prepend_i16_be(&mut self, n: i16) {
        self.prepend_array(n.to_be_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_u32_be`.
    #[inline]
    fn prepend_u32_be(&mut self, n: u32) {
        self.prepend_array(n.to_be_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_i32_be`.
    #[inline]
    fn prepend_i32_be(&mut self, n: i32) {
        self.prepend_array(n.to_be_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_u64_be`.
    #[inline]
    fn prepend_u64_be(&mut self, n: u64) {
        self.prepend_array(n.to_be_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_i64_be`.
    #[inline]
    fn prepend_i64_be(&mut self, n: i64) {
        self.prepend_array(n.to_be_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_f32_be`.
    #[inline]
    fn prepend_f32_be(&mut self, n: f32) {
        self.prepend_array(n.to_be_bytes())
    }

    /// Corresponding prepending method to `bytes::BufMut::put_f64_be`.
    #[inline]
    fn prepend_f64_be(&mut self, n: f64) {
        self.prepend_array(n.to_be_bytes())
    }
}

/// A `bytes`-compatible, exponentially-growing, prepend-only byte buffer.
///
/// `ReverseBuf` is rope-like in that it stores its data non-contiguously, but does not (yet)
/// support any rope-like operations.
#[derive(Clone)]
pub struct ReverseBuffer {
    /// Chunks of owned items in reverse order.
    #[cfg(not(feature = "forbid-unsafe"))]
    chunks: Vec<Box<[MaybeUninit<u8>]>>,
    /// Chunks of owned items in reverse order.
    #[cfg(feature = "forbid-unsafe")]
    chunks: Vec<Box<[u8]>>,
    /// Index of the first initialized byte in the front chunk (at the end of `self.chunks`).
    /// Invariant: Always a valid index in that chunk when any chunk exists, or the same as the
    /// length of the only chunk when that chunk is being kept and the buffer is empty.
    front: usize,
    /// Total size of owned bytes in the chunks, including the uninitialized bytes at the front of
    /// the front chunk.
    capacity: usize,
    /// Advisory size value for when the next chunk is allocated. If this value is positive it is an
    /// exact size for the next allocation(s); otherwise it is a negated minimum added capacity that
    /// was requested.
    planned_allocation: usize,
    /// Whether the planned allocation will be of an exact size. If false, planned_allocation is
    /// only a minimum.
    planned_exact: bool,
    /// When true, this buffer will never drop its first chunk (which is kept as reserved minimum
    /// capacity).
    keep_back: bool,
    _phantom_data: PhantomData<u8>,
}

impl ReverseBuffer {
    /// Creates a new empty buffer. This buffer will not allocate until data is added.
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            front: 0,
            capacity: 0,
            planned_allocation: 0,
            planned_exact: false,
            keep_back: false,
            _phantom_data: PhantomData,
        }
    }

    /// Creates a new buffer with a given base capacity. If the capacity is nonzero, it will always
    /// retain at least that much capacity even when fully read or cleared.
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::new();
        }
        Self {
            chunks: vec![Self::allocate_chunk(capacity)],
            front: capacity,
            capacity,
            planned_allocation: 0,
            planned_exact: false,
            keep_back: true,
            _phantom_data: PhantomData,
        }
    }

    /// Returns the number of bytes written into this buffer so far.
    #[inline]
    pub fn len(&self) -> usize {
        // Front should be zero any time the buf is empty
        debug_assert!(!(self.chunks.is_empty() && self.front > 0));
        // If there are no chunks capacity should also be zero, and chunks should always have
        // nonzero size.
        debug_assert_eq!(self.chunks.is_empty(), self.capacity == 0);
        self.capacity - self.front
    }

    /// Returns `true` if the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears the data from the buffer, including any additional allocations.
    pub fn clear(&mut self) {
        if self.keep_back {
            self.chunks.truncate(1);
            self.front = self.chunks[0].len();
            self.capacity = self.front;
            (self.planned_allocation, self.planned_exact) = (0, false);
        } else {
            *self = Self::new()
        }
    }

    /// Returns the number of bytes this buffer currently has allocated capacity for.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns a reference to the full contents of the buffer if it is fully contiguous, or None if
    /// it is not.
    pub fn contiguous(&self) -> Option<&[u8]> {
        match self.chunks.len() {
            0..=1 => Some(self.chunk()),
            _ => None,
        }
    }

    /// Converts this buffer into a single Vec. If this buffer is contiguous and has no spare
    /// capacity, the conversion will be performed without copying data.
    pub fn into_vec(mut self) -> Vec<u8> {
        if self.chunks.len() == 1 && self.front == 0 {
            let whole_buffer: Box<[u8]> = {
                #[cfg(not(feature = "forbid-unsafe"))]
                // SAFETY: the whole buffer is initialized.
                unsafe {
                    transmute(self.chunks.pop().unwrap())
                }
                #[cfg(feature = "forbid-unsafe")]
                {
                    self.chunks.pop().unwrap()
                }
            };
            // CORRECTNESS: the whole buffer is only one chunk.
            return whole_buffer.to_vec();
        }
        let mut res = Vec::with_capacity(self.len());
        bytes::BufMut::put(&mut res, self);
        res
    }

    /// Ensures that the buffer will, upon its next allocation, reserve at least enough space to fit
    /// this many more bytes than are currently in the buffer.
    #[inline(always)]
    pub fn plan_reservation(&mut self, additional: usize) {
        let Some(more_needed) = additional.checked_sub(self.front) else {
            return; // There is already enough capacity for `additional` more bytes.
        };
        if self.planned_allocation > more_needed {
            return; // Next planned allocation is already greater than the requested amount.
        }
        (self.planned_allocation, self.planned_exact) = (more_needed, false);
    }

    /// Ensures that the buffer will, upon its next allocation, reserve enough space to fit this
    /// many more bytes than are in the buffer at present. If there is already enough additional
    /// capacity to fit this many more bytes, this method has no effect. If the requested capacity
    /// is not already met and there is already a set plan for the size of the next allocation, it
    /// will be overridden by this request.
    ///
    /// If this method is repeatedly called interleaved with calls to `prepend` that trigger new
    /// allocations, the buffer may become very fragmented as this method can be used to control the
    /// exact sizes of all its allocations. Use sparingly.
    #[inline]
    pub fn plan_reservation_exact(&mut self, additional: usize) {
        let Some(more_needed) = additional.checked_sub(self.front) else {
            return;
        };
        (self.planned_allocation, self.planned_exact) = (self.capacity + more_needed, true);
    }

    /// Returns the slice of bytes ordered at the front of the buffer.
    #[cfg(not(feature = "forbid-unsafe"))]
    #[inline]
    fn front_chunk_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        self.chunks.last_mut().map(Box::as_mut).unwrap_or(&mut [])
    }
    #[cfg(feature = "forbid-unsafe")]
    fn front_chunk_mut(&mut self) -> &mut [u8] {
        self.chunks.last_mut().map(Box::as_mut).unwrap_or(&mut [])
    }

    #[cfg(not(feature = "forbid-unsafe"))]
    #[inline]
    fn allocate_chunk(new_chunk_size: usize) -> Box<[MaybeUninit<u8>]> {
        debug_assert!(new_chunk_size > 0);
        unsafe {
            let new_allocation =
                alloc::alloc::alloc(alloc::alloc::Layout::array::<u8>(new_chunk_size).unwrap())
                    as *mut MaybeUninit<u8>;
            Box::from_raw(ptr::slice_from_raw_parts_mut(
                new_allocation,
                new_chunk_size,
            ))
        }
    }
    #[cfg(feature = "forbid-unsafe")]
    #[inline]
    fn allocate_chunk(new_chunk_size: usize) -> Box<[u8]> {
        debug_assert!(new_chunk_size > 0);
        vec![0u8; new_chunk_size].into_boxed_slice()
    }

    #[inline]
    fn new_allocation_size(&self) -> usize {
        if self.planned_exact {
            // We planned an explicit exact size for the next allocation.
            debug_assert_ne!(self.planned_allocation, 0);
            self.planned_allocation
        } else {
            // We planned a minimum size for the new chunk. Choose the actual size for that
            // allocation, planning to at least double in size.
            max(max(MIN_CHUNK_SIZE, self.capacity), self.planned_allocation)
        }
    }

    /// Allocates a new chunk, adding it to `chunks` and adding its length to `front`. The front
    /// offset will need to be corrected after this call so it refers to a valid index inside this
    /// new front chunk in order to maintain invariants unless the buffer was completely empty.
    #[inline(never)]
    #[cold]
    fn grow_slow(&mut self) {
        self.grow();
    }

    #[inline]
    fn grow(&mut self) {
        let new_chunk_size = self.new_allocation_size();
        self.chunks.push(Self::allocate_chunk(new_chunk_size)); // new_chunk becomes the new front chunk
        self.front += new_chunk_size; // front now points to the first initialized index there
        self.capacity += new_chunk_size;
        // Reset the planned allocation
        (self.planned_allocation, self.planned_exact) = (0, false);
    }

    #[inline(never)]
    #[cold]
    fn grow_and_copy_buf<B: Buf>(&mut self, mut data: B, prepending_len: usize) {
        self.plan_reservation(prepending_len);

        // Create the ownership of the new chunk
        let new_chunk_size = self.new_allocation_size();
        let mut new_chunk = Self::allocate_chunk(new_chunk_size);

        // Copy bytes from the provided buffer into our two slices, the early half at the end of
        // the new chunk we just allocated, and the rest at the beginning of the "current" front
        // chunk.
        let old_front = self.front;
        let new_front = new_chunk_size + self.front - prepending_len;
        debug_assert!(new_front < new_chunk.len());
        copy_buf(&mut data, {
            #[cfg(not(feature = "forbid-unsafe"))]
            unsafe {
                new_chunk.get_unchecked_mut(new_front..)
            }
            #[cfg(feature = "forbid-unsafe")]
            {
                new_chunk.get_mut(new_front..).unwrap()
            }
        });
        debug_assert!(self
            .chunks
            .last()
            .map_or(true, |old_front_chunk| old_front < old_front_chunk.len()));
        copy_buf(&mut data, {
            #[cfg(not(feature = "forbid-unsafe"))]
            unsafe {
                self.front_chunk_mut().get_unchecked_mut(..old_front)
            }
            #[cfg(feature = "forbid-unsafe")]
            {
                &mut self.front_chunk_mut()[..old_front]
            }
        });
        debug_assert_eq!(data.remaining(), 0);
        // Data is all written; update our state.
        self.chunks.push(new_chunk); // new_chunk becomes the new front chunk
        self.front = new_front; // front now points to the first initialized index there
        self.capacity += new_chunk_size;
        // Reset the planned allocation since we already allocated
        (self.planned_allocation, self.planned_exact) = (0, false);
    }

    #[inline(never)]
    #[cold]
    fn grow_and_copy_slice(&mut self, data: &[u8]) {
        self.plan_reservation(data.len());
        let new_front;
        if self.front == 0 {
            // We are growing and copying the whole thing once into the back of the new chunk.
            self.grow();
            new_front = self.front - data.len();
            #[cfg(not(feature = "forbid-unsafe"))]
            // SAFETY: we are initializing the end of the new front chunk from `data`.
            unsafe {
                ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    &mut self.front_chunk_mut()[new_front..] as *mut _ as *mut u8,
                    data.len(),
                );
            }
            #[cfg(feature = "forbid-unsafe")]
            {
                (&mut self.front_chunk_mut()[new_front..]).put_slice(data);
            }
        } else {
            // The prepended data will be split across the current and new front chunk.
            let (data_front, data_back) = data.split_at(data.len() - self.front);
            #[cfg(not(feature = "forbid-unsafe"))]
            // SAFETY: we are initializing the range of the front chunk before the current front
            // with bytes from the back part of `data`.
            unsafe {
                ptr::copy_nonoverlapping(
                    data_back.as_ptr(),
                    self.front_chunk_mut() as *mut _ as *mut u8,
                    self.front,
                );
            }
            #[cfg(feature = "forbid-unsafe")]
            {
                self.front_chunk_mut().put_slice(data_back);
            }
            // add a new chunk
            self.grow();
            new_front = self.front - data.len();
            debug_assert_eq!(new_front + data_front.len(), self.front_chunk_mut().len());
            #[cfg(not(feature = "forbid-unsafe"))]
            // SAFETY: we are initializing the end of the new front chunk from the front part of
            // `data`.
            unsafe {
                ptr::copy_nonoverlapping(
                    data_front.as_ptr(),
                    &mut self.front_chunk_mut()[new_front..] as *mut _ as *mut u8,
                    data_front.len(),
                );
            }
            #[cfg(feature = "forbid-unsafe")]
            {
                (&mut self.front_chunk_mut()[new_front..]).put_slice(data_front);
            }
        }
        self.front = new_front;
    }

    /// Returns a reader that references this buf's contents, which implements `bytes::Buf` without
    /// draining bytes from the buffer.
    pub fn buf_reader(&self) -> ReverseBufferReader<'_> {
        ReverseBufferReader {
            chunks: self.chunks.as_slice(),
            front: self.front,
            capacity: self.capacity,
        }
    }

    /// Returns an iterator over the slices of data in the buffer, suitable for vectored writing.
    ///
    /// This can be used like so in conjunction with [`write_vectored`](
    /// https://doc.rust-lang.org/stable/std/io/trait.Write.html#method.write_vectored) (or
    /// [`write_all_vectored`](
    /// https://doc.rust-lang.org/stable/std/io/trait.Write.html#method.write_all_vectored), first
    /// collecting the slices like `b.slices().map(IoSlice::new)`.
    pub fn slices(&self) -> impl Iterator<Item = &[u8]> {
        #[cfg(not(feature = "forbid-unsafe"))]
        {
            // SAFETY: self is valid
            unsafe { to_vectorable_slices(&self.chunks, self.front) }
        }
        #[cfg(feature = "forbid-unsafe")]
        {
            // this version of `to_vectorable_slices` is safe
            to_vectorable_slices(&self.chunks, self.front)
        }
    }
}

/// Copies bytes out of a `bytes::Buf` directly into a slice of uninitialized bytes, filling it. The
/// source must have enough bytes to fill the destination.
#[cfg(not(feature = "forbid-unsafe"))]
#[inline(always)]
fn copy_buf<B: Buf>(data: &mut B, mut dest_chunk: &mut [MaybeUninit<u8>]) {
    debug_assert!(data.remaining() >= dest_chunk.len());
    while !dest_chunk.is_empty() {
        let src = data.chunk();
        let copy_size = min(src.len(), dest_chunk.len());
        // SAFETY: we are initializing dest_chunk with bytes from `data`.
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), dest_chunk.as_mut_ptr() as *mut u8, copy_size);
        }
        dest_chunk = &mut dest_chunk[copy_size..];
        data.advance(copy_size);
    }
}
/// Copies bytes out of a `bytes::Buf` directly into a slice of bytes, filling it. The source must
/// have enough bytes to fill the destination.
#[cfg(feature = "forbid-unsafe")]
#[inline(always)]
fn copy_buf<B: Buf>(data: &mut B, mut dest_chunk: &mut [u8]) {
    debug_assert!(data.remaining() >= dest_chunk.len());
    while !dest_chunk.is_empty() {
        let src = data.chunk();
        let copy_size = min(src.len(), dest_chunk.len());
        dest_chunk.put_slice(&src[..copy_size]);
        data.advance(copy_size);
    }
}

#[cfg(not(feature = "forbid-unsafe"))]
// SAFETY: front must be a valid front index in the front chunk.
#[inline(always)]
unsafe fn to_vectorable_slices(
    chunks: &[Box<[MaybeUninit<u8>]>],
    front: usize,
) -> impl Iterator<Item = &[u8]> {
    chunks
        .split_last()
        .into_iter()
        .flat_map(move |(front_chunk, rest)| {
            iter::once(&front_chunk[front..]).chain(rest.iter().rev().map(Box::as_ref))
        })
        // SAFETY: the portion of the front chunk after `front` that we have sliced, and every chunk
        // after that, are all valid bytes.
        .map(|m| unsafe { transmute::<&[MaybeUninit<u8>], &[u8]>(m) })
}
#[cfg(feature = "forbid-unsafe")]
#[inline(always)]
fn to_vectorable_slices(chunks: &[Box<[u8]>], front: usize) -> impl Iterator<Item = &[u8]> {
    chunks
        .split_last()
        .into_iter()
        .flat_map(move |(front_chunk, rest)| {
            iter::once(&front_chunk[front..]).chain(rest.iter().rev().map(Box::as_ref))
        })
}

impl ReverseBuf for ReverseBuffer {
    #[inline(always)]
    fn prepend<B: Buf>(&mut self, mut data: B) {
        let prepending_len = data.remaining();
        if prepending_len == 0 {
            return;
        }
        if prepending_len <= self.front {
            // The data fits in our current front chunk; copy it into there and update the front
            // index.
            let new_front = self.front - prepending_len;
            if ENABLE_SELF_COPY_OPTIMIZATION && prepending_len <= MAX_SELF_COPY {
                let mut data_to_slice = [0; MAX_SELF_COPY];
                data.copy_to_slice(&mut data_to_slice[..prepending_len]);
                let old_front = self.front;
                for (from, to) in data_to_slice[..prepending_len]
                    .iter()
                    .rev()
                    .zip(self.front_chunk_mut()[..old_front].iter_mut().rev())
                {
                    #[cfg(not(feature = "forbid-unsafe"))]
                    {
                        *to = MaybeUninit::new(*from);
                    }
                    #[cfg(feature = "forbid-unsafe")]
                    {
                        *to = *from;
                    }
                }
            } else {
                let dest_range = new_front..self.front;
                // We have a nonzero `front`, therefore we must have a front chunk.
                copy_buf(&mut data, &mut self.front_chunk_mut()[dest_range]);
                // Copy each chunk of `data` into this uninitialized destination.
                debug_assert_eq!(data.remaining(), 0);
            }
            self.front = new_front; // Those bytes are now initialized.
        } else {
            self.grow_and_copy_buf(data, prepending_len);
        }
    }

    fn remaining_writable(&self) -> usize {
        // ReverseBuf only fails when allocating does.
        usize::MAX - self.capacity
    }

    #[inline(always)]
    fn prepend_slice(&mut self, data: &[u8]) {
        if let Some(new_front) = self.front.checked_sub(data.len()) {
            if ENABLE_SELF_COPY_OPTIMIZATION && data.len() <= MAX_SELF_COPY {
                // For ?reasons?, doing this copy backwards is way, way, way faster on many x86 cpus
                let old_front = self.front;
                for (from, to) in data
                    .iter()
                    .rev()
                    .zip(self.front_chunk_mut()[..old_front].iter_mut().rev())
                {
                    #[cfg(not(feature = "forbid-unsafe"))]
                    {
                        *to = MaybeUninit::new(*from);
                    }
                    #[cfg(feature = "forbid-unsafe")]
                    {
                        *to = *from;
                    }
                }
            } else {
                #[cfg(not(feature = "forbid-unsafe"))]
                // SAFETY: we are initializing the range of the front chunk before the front with
                // bytes from `data`.
                unsafe {
                    ptr::copy_nonoverlapping(
                        data.as_ptr(),
                        self.front_chunk_mut()[new_front..].as_mut_ptr() as *mut u8,
                        data.len(),
                    );
                }
                #[cfg(feature = "forbid-unsafe")]
                {
                    (&mut self.front_chunk_mut()[new_front..]).put_slice(data);
                }
            }
            self.front = new_front;
        } else {
            self.grow_and_copy_slice(data);
        }
    }

    /// This is the call we dispatch to for fixed-size values, like f32 and so on. This path never
    /// enables the self-copy optimization, and is significantly faster for small fixed-size values
    /// even while variably-sized varints of the same size can be significantly slower without the
    /// self-copy optimization.
    #[inline(always)]
    fn prepend_array<const N: usize>(&mut self, data: [u8; N]) {
        if let Some(new_front) = self.front.checked_sub(N) {
            #[cfg(not(feature = "forbid-unsafe"))]
            // SAFETY: we are initializing the range of the front chunk before the front with
            // bytes from `data`.
            unsafe {
                ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    self.front_chunk_mut()[new_front..].as_mut_ptr() as *mut u8,
                    data.len(),
                );
            }
            #[cfg(feature = "forbid-unsafe")]
            {
                (&mut self.front_chunk_mut()[new_front..]).put_slice(&data);
            }
            self.front = new_front;
        } else {
            self.grow_and_copy_slice(&data);
        }
    }

    #[inline(always)]
    fn prepend_u8(&mut self, byte: u8) {
        if self.front == 0 {
            self.grow_slow();
        }
        let new_front = self.front - 1;
        #[cfg(not(feature = "forbid-unsafe"))]
        {
            self.front_chunk_mut()[new_front].write(byte);
        }
        #[cfg(feature = "forbid-unsafe")]
        {
            *self.front_chunk_mut().get_mut(new_front).unwrap() = byte;
        }
        self.front = new_front;
    }
}

impl Default for ReverseBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// The implementation of `bytes::Buf` for `ReverseBuf` drains bytes from the buffer as they are
/// advanced past.
impl Buf for ReverseBuffer {
    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline(always)]
    fn chunk(&self) -> &[u8] {
        let Some(front_chunk) = self.chunks.last() else {
            return &[];
        };
        debug_assert!(self.front < front_chunk.len());
        #[cfg(not(feature = "forbid-unsafe"))]
        // SAFETY: front is always a valid index in the front chunk, and the bytes at and
        // after that index are always initialized.
        unsafe {
            transmute(front_chunk.get_unchecked(self.front..))
        }
        #[cfg(feature = "forbid-unsafe")]
        {
            &front_chunk[self.front..]
        }
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        if cnt == 0 {
            return;
        }
        if cnt > self.len() {
            panic!("advanced past end");
        };
        // `front` becomes the number of bytes from the front that the new front of the buffer will
        // be. This temporarily breaks our invariant that `front` must always be a valid index in
        // the front chunk, so we will be removing chunks and subtracting from front until it fits.
        self.front += cnt;
        // Pop chunks off the front of the buffer until the new front doesn't overflow the front
        // chunk
        while let Some(front_chunk) = self.chunks.last() {
            let front_chunk_size = front_chunk.len();
            if self.front < front_chunk_size {
                break;
            }
            // If we have exactly one chunk left and we are retaining the back chunk, don't drop it.
            if self.keep_back && self.chunks.len() == 1 {
                debug_assert!(self.capacity == self.front);
                break;
            }
            drop(self.chunks.pop());
            self.capacity -= front_chunk_size;
            self.front -= front_chunk_size;
            // Now that the buffer has shrunk, unplan any future allocations.
            (self.planned_allocation, self.planned_exact) = (0, false);
        }
    }
}

impl From<ReverseBuffer> for Vec<u8> {
    fn from(value: ReverseBuffer) -> Self {
        value.into_vec()
    }
}

impl From<Box<[u8]>> for ReverseBuffer {
    fn from(value: Box<[u8]>) -> Self {
        Self {
            capacity: value.len(),
            // SAFETY: we are actually marking the data as LESS initialized, in a transparent
            // representation. This is trivial to do and completely safe, but there's no safe method
            // available to call that does this for you itemwise when it's in a Box like this.
            chunks: vec![{
                #[cfg(not(feature = "forbid-unsafe"))]
                unsafe {
                    transmute::<Box<[u8]>, Box<[MaybeUninit<u8>]>>(value)
                }
                #[cfg(feature = "forbid-unsafe")]
                {
                    value
                }
            }],
            front: 0, // front chunk is full
            ..Self::new()
        }
    }
}

impl From<Vec<u8>> for ReverseBuffer {
    fn from(value: Vec<u8>) -> Self {
        Self::from(value.into_boxed_slice())
    }
}

/// Non-draining reader-by-reference for `ReverseBuf`, implementing `bytes::Buf`.
pub struct ReverseBufferReader<'a> {
    /// Buffer being read
    #[cfg(not(feature = "forbid-unsafe"))]
    chunks: &'a [Box<[MaybeUninit<u8>]>],
    /// Buffer being read
    #[cfg(feature = "forbid-unsafe")]
    chunks: &'a [Box<[u8]>],
    /// Index of the front byte in the front chunk (the last in the slice). If chunks is non-empty,
    /// front is always a valid index inside it.
    front: usize,
    /// Total size of all the boxes covered by chunks
    capacity: usize,
}

impl ReverseBufferReader<'_> {
    /// Returns a reference to the full contents of the buffer if it is fully contiguous, or None if
    /// it is not.
    pub fn contiguous(&self) -> Option<&[u8]> {
        match self.chunks.len() {
            0..=1 => Some(self.chunk()),
            _ => None,
        }
    }

    /// Returns an iterator over the slices of data in this buffer, suitable for vectored writing.
    ///
    /// This can be used like so in conjunction with [`write_vectored`](
    /// https://doc.rust-lang.org/stable/std/io/trait.Write.html#method.write_vectored) (or
    /// [`write_all_vectored`](
    /// https://doc.rust-lang.org/stable/std/io/trait.Write.html#method.write_all_vectored), first
    /// collecting the slices like `b.slices().map(IoSlice::new)`.
    pub fn slices(&self) -> impl Iterator<Item = &[u8]> {
        #[cfg(not(feature = "forbid-unsafe"))]
        {
            // SAFETY: self is valid
            unsafe { to_vectorable_slices(self.chunks, self.front) }
        }
        #[cfg(feature = "forbid-unsafe")]
        {
            // this version of `to_vectorable_slices
            to_vectorable_slices(self.chunks, self.front)
        }
    }
}

impl Buf for ReverseBufferReader<'_> {
    #[inline]
    fn remaining(&self) -> usize {
        self.capacity - self.front
    }

    #[inline(always)]
    fn chunk(&self) -> &[u8] {
        let Some(front_chunk) = self.chunks.last() else {
            return &[];
        };
        debug_assert!(self.front < front_chunk.len());
        #[cfg(not(feature = "forbid-unsafe"))]
        // SAFETY: front is always a valid index in the front chunk, and the bytes at and
        // after that index are always initialized.
        {
            unsafe { transmute(front_chunk.get_unchecked(self.front..)) }
        }
        #[cfg(feature = "forbid-unsafe")]
        {
            &front_chunk[self.front..]
        }
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        if cnt == 0 {
            return;
        }
        if cnt > self.remaining() {
            panic!("advanced past end");
        };
        // `front` becomes the number of bytes from the front that the new front of the buffer will
        // be. This temporarily breaks our invariant that `front` must always be a valid index in
        // the front chunk, so we will be removing chunks and subtracting from front until it fits.
        self.front += cnt;
        // Pop chunks off until the new front doesn't overflow the front chunk
        while let Some(front_chunk) = self.chunks.last() {
            if self.front < front_chunk.len() {
                break;
            }
            // Snip off the first chunk from the end of the slice (chunks are in reverse order,
            // remember)
            self.chunks = &self.chunks[..self.chunks.len() - 1];
            self.front -= front_chunk.len();
            self.capacity -= front_chunk.len();
        }
    }
}

// TODO: if and when we want to also implement borrowable encoding for non-contiguous borrowed bufs,
//  we can bring this back. for now that feels like too much, as the only thing that would make use
//  of this is Cow (since we do not have implementations for any rope-like storage)
//
// pub trait BorrowBuf<'a>: Buf {
//     fn borrow_chunk(&self) -> &'a [u8];
// }
//
// impl<'a> BorrowBuf<'a> for &'a [u8] {
//     fn borrow_chunk(&self) -> &'a [u8] {
//         self
//     }
// }
//
// impl<'a, B> BorrowBuf<'a> for &'_ mut B
// where B: BorrowBuf<'a>
// {
//     fn borrow_chunk(&self) -> &'a [u8] {
//         (**self).borrow_chunk()
//     }
// }

#[cfg(test)]
mod test {
    use super::{ReverseBuf, ReverseBuffer};
    use alloc::vec::Vec;
    use bytes::{Buf, BufMut};

    fn compare_buf(buf: impl Buf, expected: &[u8]) {
        let mut read = Vec::new();
        read.put(buf);
        assert_eq!(read, expected);
    }

    #[allow(clippy::len_zero)]
    fn check_read(buf: ReverseBuffer, expected: &[u8]) {
        assert_eq!(buf.len(), expected.len());
        assert_eq!(buf.is_empty(), buf.len() == 0);

        let mut vectored = Vec::new();
        for slice in buf.slices() {
            vectored.extend_from_slice(slice);
        }
        assert_eq!(vectored, expected);

        let mut vectored = Vec::new();
        for slice in buf.buf_reader().slices() {
            vectored.extend_from_slice(slice);
        }
        assert_eq!(vectored, expected);

        compare_buf(buf.buf_reader(), expected);
        compare_buf(buf, expected);
    }

    #[test]
    fn fresh() {
        check_read(ReverseBuffer::new(), b"");
    }

    #[test]
    fn fresh_with_plan_still_empty() {
        let mut buf = ReverseBuffer::new();
        buf.plan_reservation(100);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        check_read(buf, b"");
    }

    #[test]
    fn build_and_read() {
        let mut buf = ReverseBuffer::new();
        buf.prepend_slice(b"!");
        buf.prepend_slice(b"world");
        buf.prepend_slice(b"hello ");
        check_read(buf, b"hello world!");
    }

    #[test]
    fn build_bigger_and_read() {
        let mut buf = ReverseBuffer::new();
        buf.prepend_slice(b"!");
        buf.prepend_slice(b"world");
        buf.prepend_slice(b"hello ");
        let mut buf2 = ReverseBuffer::new();
        buf2.prepend(buf.buf_reader()); // 1
        buf.prepend(buf2.buf_reader()); // 2
        buf2.prepend(buf.buf_reader()); // 3
        buf.prepend(buf2.buf_reader()); // 5
        buf2.prepend(buf.buf_reader()); // 8
        buf.prepend(buf2.buf_reader()); // 13
        assert_eq!(buf.chunks.len(), 3);
        check_read(
            buf.clone(),
            b"hello world!hello world!hello world!hello world!hello world!hello world!\
            hello world!hello world!hello world!hello world!hello world!hello world!hello world!",
        );
        check_read(
            buf,
            b"hello world!hello world!hello world!hello world!hello world!hello world!\
            hello world!hello world!hello world!hello world!hello world!hello world!hello world!",
        );
    }

    #[test]
    fn build_with_planned_reservation() {
        let mut buf = ReverseBuffer::new();
        buf.prepend_slice(b"!");
        buf.prepend_slice(b"world");
        buf.prepend_slice(b"hello ");
        buf.plan_reservation(b"hello world!".len() * 12);
        let mut buf2 = ReverseBuffer::new();
        buf2.prepend(buf.buf_reader()); // 1
        buf.prepend(buf2.buf_reader()); // 2
        buf2.prepend(buf.buf_reader()); // 3
        buf.prepend(buf2.buf_reader()); // 5
        buf2.prepend(buf.buf_reader()); // 8
        buf.prepend(buf2.buf_reader()); // 13
                                        // Only one additional chunk was allocated
        assert_eq!(buf.chunks.len(), 2);
        // No extra capacity exists in the buffer at this point
        assert_eq!(buf.capacity() - buf.len(), 0);
        check_read(
            buf,
            b"hello world!hello world!hello world!hello world!hello world!hello world!\
            hello world!hello world!hello world!hello world!hello world!hello world!hello world!",
        );
    }

    #[test]
    fn build_with_initial_planned_reservation() {
        let mut buf = ReverseBuffer::new();
        buf.plan_reservation(b"hello world!".len() * 13);
        buf.prepend_slice(b"!");
        buf.prepend_slice(b"world");
        buf.prepend_slice(b"hello ");
        let mut buf2 = ReverseBuffer::new();
        buf2.prepend(buf.buf_reader()); // 1
        buf.prepend(buf2.buf_reader()); // 2
        buf2.prepend(buf.buf_reader()); // 3
        buf.prepend(buf2.buf_reader()); // 5
        buf2.prepend(buf.buf_reader()); // 8
        buf.prepend(buf2.buf_reader()); // 13
        assert_eq!(buf.chunks.len(), 1); // Only one chunk was allocated in total
        assert_eq!(buf.capacity() - buf.len(), 0); // No extra capacity exists in the buffer
        check_read(
            buf,
            b"hello world!hello world!hello world!hello world!hello world!hello world!\
            hello world!hello world!hello world!hello world!hello world!hello world!hello world!",
        );
    }

    #[test]
    fn build_with_exact_planned_reservation() {
        let mut buf = ReverseBuffer::new();
        buf.plan_reservation_exact(b"hello world!".len());
        buf.prepend_slice(b"!");
        buf.prepend_slice(b"world");
        buf.prepend_slice(b"hello ");
        assert_eq!(buf.chunks.len(), 1); // Only one chunk was allocated in total
        assert_eq!(buf.capacity() - buf.len(), 0); // No extra capacity exists in the buffer
        check_read(buf, b"hello world!");
    }

    #[test]
    fn build_with_capacity() {
        let mut buf = ReverseBuffer::with_capacity(b"hello world!".len());
        assert_eq!(buf.capacity(), b"hello world!".len());
        assert!(buf.is_empty());
        assert_eq!(buf.chunks.len(), 1);
        buf.prepend_slice(b"!");
        buf.prepend_slice(b"world");
        buf.prepend_slice(b"hello ");
        assert_eq!(buf.capacity(), b"hello world!".len());
        assert_eq!(buf.len(), buf.capacity());
        assert_eq!(buf.chunks.len(), 1);
        buf.prepend_slice(b"12345");
        assert_eq!(buf.chunks.len(), 2);
        assert!(buf.capacity() > buf.len());
        check_read(buf, b"12345hello world!");
    }

    #[test]
    fn single_prepend_allocates_once() {
        let mut buf = ReverseBuffer::with_capacity(4);
        buf.prepend_slice(b"aa");
        buf.prepend_slice(&[0; 100]);
        assert_eq!(buf.len(), 102);
        assert_eq!(buf.capacity(), buf.len());
        assert_eq!(buf.chunks.len(), 2);
        buf.clear();
        assert_eq!(buf.chunks.len(), 1);
        assert_eq!(buf.capacity(), 4);
        assert!(buf.is_empty());
    }

    #[test]
    fn fully_advancing_keep_back_reversebuffer_does_not_drop_back() {
        let mut buf = ReverseBuffer::with_capacity(4);
        buf.prepend_slice(b"asdfa");
        assert_eq!(buf.chunks.len(), 2);
        assert!(buf.capacity() > 5);
        assert_eq!(buf.remaining(), 5);
        compare_buf(&mut buf, b"asdfa");
        assert_eq!(buf.capacity(), 4); // this buffer retained the back chunk

        buf = ReverseBuffer::new();
        buf.plan_reservation_exact(4);
        buf.prepend_slice(b"as");
        buf.prepend_slice(b"dfa");
        assert_eq!(buf.chunks.len(), 2);
        assert!(buf.capacity() > 5);
        assert_eq!(buf.remaining(), 5);
        compare_buf(&mut buf, b"dfaas");
        assert_eq!(buf.capacity(), 0); // this buffer doesn't retain its back chunk
    }

    #[test]
    fn prepending_small_bufs() {
        let mut buf = ReverseBuffer::new();
        buf.prepend("hello".as_bytes());
        buf.prepend("why ".as_bytes());
        check_read(buf, b"why hello");
    }
}
