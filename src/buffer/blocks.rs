//! Per-block per-channel per-sample iterators.

use std::marker::PhantomData;

#[cfg(feature = "simd")]
use std::simd::{LaneCount, Simd, SupportedLaneCount};

use super::SamplesIter;

/// An iterator over all samples in the buffer, slicing over the sample-dimension with a maximum
/// size of `max_block_size`. See [`Buffer::iter_blocks()`][super::Buffer::iter_blocks()]. Yields
/// both the block and the offset from the start of the buffer.
pub struct BlocksIter<'slice, 'sample: 'slice> {
    /// The raw output buffers.
    pub(super) buffers: *mut [&'sample mut [f32]],
    pub(super) max_block_size: usize,
    pub(super) current_block_start: usize,
    pub(super) _marker: PhantomData<&'slice mut [&'sample mut [f32]]>,
}

/// A block yielded by [`BlocksIter`]. Can be iterated over once or multiple times, and also
/// supports direct access to the block's samples if needed.
pub struct Block<'slice, 'sample: 'slice> {
    /// The raw output buffers.
    pub(self) buffers: *mut [&'sample mut [f32]],
    pub(self) current_block_start: usize,
    /// The index of the last sample in the block plus one.
    pub(self) current_block_end: usize,
    pub(self) _marker: PhantomData<&'slice mut [&'sample mut [f32]]>,
}

/// An iterator over all channels in a block yielded by [`Block`], returning an entire channel slice
/// at a time.
pub struct BlockChannelsIter<'slice, 'sample: 'slice> {
    /// The raw output buffers.
    pub(self) buffers: *mut [&'sample mut [f32]],
    pub(self) current_block_start: usize,
    pub(self) current_block_end: usize,
    pub(self) current_channel: usize,
    pub(self) _marker: PhantomData<&'slice mut [&'sample mut [f32]]>,
}

impl<'slice, 'sample> Iterator for BlocksIter<'slice, 'sample> {
    type Item = (usize, Block<'slice, 'sample>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let buffer_len = unsafe { (*self.buffers).first().map(|b| b.len()).unwrap_or(0) };
        if self.current_block_start < buffer_len {
            let current_block_start = self.current_block_start;
            let current_block_end =
                (self.current_block_start + self.max_block_size).min(buffer_len);
            let block = Block {
                buffers: self.buffers,
                current_block_start,
                current_block_end,
                _marker: self._marker,
            };

            self.current_block_start += self.max_block_size;

            Some((current_block_start, block))
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let buffer_len = unsafe { (*self.buffers).first().map(|b| b.len()).unwrap_or(0) };
        let remaining = (buffer_len as f32 / self.max_block_size as f32).ceil() as usize;

        (remaining, Some(remaining))
    }
}

impl<'slice, 'sample> IntoIterator for Block<'slice, 'sample> {
    type Item = &'sample mut [f32];
    type IntoIter = BlockChannelsIter<'slice, 'sample>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        BlockChannelsIter {
            buffers: self.buffers,
            current_block_start: self.current_block_start,
            current_block_end: self.current_block_end,
            current_channel: 0,
            _marker: self._marker,
        }
    }
}

impl<'slice, 'sample> Iterator for BlockChannelsIter<'slice, 'sample> {
    type Item = &'sample mut [f32];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_channel < unsafe { (&(*self.buffers)).len() } {
            // SAFETY: These bounds have already been checked
            // SAFETY: It is also not possible to have multiple mutable references to the same
            //         sample at the same time
            let slice = unsafe {
                (&mut (*self.buffers))
                    .get_unchecked_mut(self.current_channel)
                    .get_unchecked_mut(self.current_block_start..self.current_block_end)
            };

            self.current_channel += 1;

            Some(slice)
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = unsafe { (&(*self.buffers)).len() } - self.current_channel;

        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for BlocksIter<'_, '_> {}
impl ExactSizeIterator for BlockChannelsIter<'_, '_> {}

impl<'slice, 'sample> Block<'slice, 'sample> {
    /// Get the number of samples per channel in the block.
    #[inline]
    pub fn samples(&self) -> usize {
        self.current_block_end - self.current_block_start
    }

    /// Returns the number of channels in this buffer.
    #[inline]
    pub fn channels(&self) -> usize {
        unsafe { (&(*self.buffers)).len() }
    }

    /// A resetting iterator. This lets you iterate over the same block multiple times. Otherwise
    /// you don't need to use this function as [`Block`] already implements [`Iterator`]. You can
    /// also use the direct accessor functions on this block instead.
    #[inline]
    pub fn iter_mut(&mut self) -> BlockChannelsIter<'slice, 'sample> {
        BlockChannelsIter {
            buffers: self.buffers,
            current_block_start: self.current_block_start,
            current_block_end: self.current_block_end,
            current_channel: 0,
            _marker: self._marker,
        }
    }

    /// Iterate over this block on a per-sample per-channel basis. This is identical to
    /// [`Buffer::iter_samples()`][super::Buffer::iter_samples()] but for a smaller block instead of
    /// the entire buffer
    #[inline]
    pub fn iter_samples(&mut self) -> SamplesIter<'slice, 'sample> {
        SamplesIter {
            buffers: self.buffers,
            current_sample: self.current_block_start,
            samples_end: self.current_block_end,
            _marker: self._marker,
        }
    }

    /// Access a channel by index. Useful when you would otherwise iterate over this [`Block`]
    /// multiple times.
    #[inline]
    pub fn get(&self, channel_index: usize) -> Option<&[f32]> {
        // SAFETY: The block bound has already been checked
        unsafe {
            Some(
                (&(*self.buffers))
                    .get(channel_index)?
                    .get_unchecked(self.current_block_start..self.current_block_end),
            )
        }
    }

    /// The same as [`get()`][Self::get], but without any bounds checking.
    ///
    /// # Safety
    ///
    /// `channel_index` must be in the range `0..Self::len()`.
    #[inline]
    pub unsafe fn get_unchecked(&self, channel_index: usize) -> &[f32] {
        (&(*self.buffers))
            .get_unchecked(channel_index)
            .get_unchecked(self.current_block_start..self.current_block_end)
    }

    /// Access a mutable channel by index. Useful when you would otherwise iterate over this
    /// [`Block`] multiple times.
    #[inline]
    pub fn get_mut(&mut self, channel_index: usize) -> Option<&mut [f32]> {
        // SAFETY: The block bound has already been checked
        unsafe {
            Some(
                (&mut (*self.buffers))
                    .get_mut(channel_index)?
                    .get_unchecked_mut(self.current_block_start..self.current_block_end),
            )
        }
    }

    /// The same as [`get_mut()`][Self::get_mut], but without any bounds checking.
    ///
    /// # Safety
    ///
    /// `channel_index` must be in the range `0..Self::len()`.
    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, channel_index: usize) -> &mut [f32] {
        (&mut (*self.buffers))
            .get_unchecked_mut(channel_index)
            .get_unchecked_mut(self.current_block_start..self.current_block_end)
    }

    /// Get a SIMD vector containing the channel data for a specific sample in this block. If `LANES
    /// > channels.len()` then this will be padded with zeroes. If `LANES < channels.len()` then
    /// this won't contain all values.
    ///
    /// Returns a `None` value if `sample_index` is out of bounds.
    #[cfg(feature = "simd")]
    #[inline]
    pub fn to_channel_simd<const LANES: usize>(
        &self,
        sample_index: usize,
    ) -> Option<Simd<f32, LANES>>
    where
        LaneCount<LANES>: SupportedLaneCount,
    {
        if sample_index > self.samples() {
            return None;
        }

        let used_lanes = self.samples().max(LANES);
        let mut values = [0.0; LANES];
        for (channel_idx, value) in values.iter_mut().enumerate().take(used_lanes) {
            *value = unsafe {
                *(&(*self.buffers))
                    .get_unchecked(channel_idx)
                    .get_unchecked(self.current_block_start + sample_index)
            };
        }

        Some(Simd::from_array(values))
    }

    /// Get a SIMD vector containing the channel data for a specific sample in this block. Will
    /// always read exactly `LANES` channels, and does not perform bounds checks on `sample_index`.
    ///
    /// # Safety
    ///
    /// Undefined behavior if `LANES > block.len()` or if `sample_index > block.len()`.
    #[cfg(feature = "simd")]
    #[inline]
    pub unsafe fn to_channel_simd_unchecked<const LANES: usize>(
        &self,
        sample_index: usize,
    ) -> Simd<f32, LANES>
    where
        LaneCount<LANES>: SupportedLaneCount,
    {
        let mut values = [0.0; LANES];
        for (channel_idx, value) in values.iter_mut().enumerate() {
            *value = *(&(*self.buffers))
                .get_unchecked(channel_idx)
                .get_unchecked(self.current_block_start + sample_index);
        }

        Simd::from_array(values)
    }

    /// Write data from a SIMD vector to this sample's channel data for a specific sample in this
    /// block. This takes the padding added by [`to_channel_simd()`][Self::to_channel_simd()] into
    /// account.
    ///
    /// Returns `false` if `sample_index` is out of bounds.
    #[cfg(feature = "simd")]
    #[allow(clippy::wrong_self_convention)]
    #[inline]
    pub fn from_channel_simd<const LANES: usize>(
        &mut self,
        sample_index: usize,
        vector: Simd<f32, LANES>,
    ) -> bool
    where
        LaneCount<LANES>: SupportedLaneCount,
    {
        if sample_index > self.samples() {
            return false;
        }

        let used_lanes = self.samples().max(LANES);
        let values = vector.to_array();
        for (channel_idx, value) in values.into_iter().enumerate().take(used_lanes) {
            *unsafe {
                (&mut (*self.buffers))
                    .get_unchecked_mut(channel_idx)
                    .get_unchecked_mut(self.current_block_start + sample_index)
            } = value;
        }

        true
    }

    /// Write data from a SIMD vector to this sample's channel data for a specific sample in this
    /// block.. This assumes `LANES` matches exactly with the number of channels in the buffer, and
    /// does not perform bounds checks on `sample_index`.
    ///
    /// # Safety
    ///
    /// Undefined behavior if `LANES > block.len()` or if `sample_index > block.len()`.
    #[cfg(feature = "simd")]
    #[allow(clippy::wrong_self_convention)]
    #[inline]
    pub unsafe fn from_channel_simd_unchecked<const LANES: usize>(
        &mut self,
        sample_index: usize,
        vector: Simd<f32, LANES>,
    ) where
        LaneCount<LANES>: SupportedLaneCount,
    {
        let values = vector.to_array();
        for (channel_idx, value) in values.into_iter().enumerate() {
            *(&mut (*self.buffers))
                .get_unchecked_mut(channel_idx)
                .get_unchecked_mut(self.current_block_start + sample_index) = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::Buffer;

    /// Helper function to create a test buffer with the given number of channels and samples
    fn create_test_buffer(num_channels: usize, num_samples: usize) -> (Buffer<'static>, Vec<Vec<f32>>) {
        let mut real_buffers = vec![vec![0.0; num_samples]; num_channels];
        let mut buffer = Buffer::default();

        unsafe {
            // SAFETY: We're creating test data that will live for the duration of the test
            // We use transmute to extend the lifetime, which is safe in test context
            let real_buffers_ptr = real_buffers.as_mut_ptr();
            buffer.set_slices(num_samples, |output_slices| {
                output_slices.clear();
                for i in 0..num_channels {
                    let channel_ptr = real_buffers_ptr.add(i);
                    let channel_slice: &mut [f32] = &mut (**channel_ptr)[..];
                    output_slices.push(std::mem::transmute::<&mut [f32], &'static mut [f32]>(channel_slice));
                }
            });
        }

        (buffer, real_buffers)
    }

    // BlocksIter tests
    #[test]
    fn blocks_iter_count() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 100);
        let max_block_size = 32;

        let count = buffer.iter_blocks(max_block_size).count();

        // 100 samples with max_block_size 32 should give 4 blocks (32, 32, 32, 4)
        assert_eq!(count, 4);
    }

    #[test]
    fn blocks_iter_sizes() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 100);
        let max_block_size = 32;

        let blocks: Vec<usize> = buffer.iter_blocks(max_block_size)
            .map(|(_, block)| block.samples())
            .collect();

        // First 3 blocks should be 32 samples, last should be 4
        assert_eq!(blocks, vec![32, 32, 32, 4]);
    }

    #[test]
    fn blocks_iter_offsets() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 100);
        let max_block_size = 32;

        let offsets: Vec<usize> = buffer.iter_blocks(max_block_size)
            .map(|(offset, _)| offset)
            .collect();

        // Offsets should be sequential
        assert_eq!(offsets, vec![0, 32, 64, 96]);
    }

    #[test]
    fn blocks_iter_size_hint() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 100);
        let max_block_size = 32;

        let iter = buffer.iter_blocks(max_block_size);
        let (lower, upper) = iter.size_hint();

        assert_eq!(lower, 4);
        assert_eq!(upper, Some(4));
    }

    #[test]
    fn blocks_iter_exact_size() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 100);
        let max_block_size = 32;

        let iter = buffer.iter_blocks(max_block_size);

        assert_eq!(iter.len(), 4);
    }

    #[test]
    fn blocks_iter_single_block() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 16);
        let max_block_size = 32;

        let blocks: Vec<(usize, usize)> = buffer.iter_blocks(max_block_size)
            .map(|(offset, block)| (offset, block.samples()))
            .collect();

        // Buffer smaller than max_block_size should give single block
        assert_eq!(blocks, vec![(0, 16)]);
    }

    #[test]
    fn blocks_iter_exact_fit() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 32);
        let max_block_size = 32;

        let blocks: Vec<(usize, usize)> = buffer.iter_blocks(max_block_size)
            .map(|(offset, block)| (offset, block.samples()))
            .collect();

        // Buffer exactly matching max_block_size
        assert_eq!(blocks, vec![(0, 32)]);
    }

    #[test]
    fn blocks_iter_perfect_division() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 128);
        let max_block_size = 32;

        let blocks: Vec<(usize, usize)> = buffer.iter_blocks(max_block_size)
            .map(|(offset, block)| (offset, block.samples()))
            .collect();

        // 128 / 32 = 4 perfect blocks
        assert_eq!(blocks, vec![(0, 32), (32, 32), (64, 32), (96, 32)]);
    }

    #[test]
    fn blocks_iter_empty_buffer() {
        let (mut buffer, _real_buffers) = create_test_buffer(0, 0);
        let max_block_size = 32;

        let count = buffer.iter_blocks(max_block_size).count();

        assert_eq!(count, 0);
    }

    // Block accessor tests
    #[test]
    fn block_get_bounds_checking() {
        let (mut buffer, _real_buffers) = create_test_buffer(2, 64);
        let max_block_size = 32;

        let mut blocks = buffer.iter_blocks(max_block_size);
        let (_, block) = blocks.next().unwrap();

        // Valid channel indices
        assert!(block.get(0).is_some());
        assert!(block.get(1).is_some());

        // Invalid channel index
        assert!(block.get(2).is_none());
        assert!(block.get(100).is_none());
    }

    #[test]
    fn block_samples_channels() {
        let (mut buffer, _real_buffers) = create_test_buffer(3, 64);
        let max_block_size = 32;

        let mut blocks = buffer.iter_blocks(max_block_size);
        let (_, block) = blocks.next().unwrap();

        assert_eq!(block.channels(), 3);
        assert_eq!(block.samples(), 32);

        // Second block should have same channels but potentially different samples
        let (_, block2) = blocks.next().unwrap();
        assert_eq!(block2.channels(), 3);
        assert_eq!(block2.samples(), 32);
    }
}
