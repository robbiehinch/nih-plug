pub struct LookaheadBuffer {
    // Ring buffers for main input (bass) - one per channel
    buffers: Vec<Vec<f32>>,
    buffer_size: usize,
    write_pos: usize,
}

impl LookaheadBuffer {
    pub fn new(num_channels: usize, buffer_size: usize) -> Self {
        Self {
            buffers: vec![vec![0.0; buffer_size]; num_channels],
            buffer_size,
            write_pos: 0,
        }
    }

    pub fn write_sample(&mut self, channel: usize, sample: f32) {
        self.buffers[channel][self.write_pos] = sample;
    }

    pub fn advance_write_pos(&mut self) {
        self.write_pos = (self.write_pos + 1) % self.buffer_size;
    }

    pub fn read_sample(&self, channel: usize, delay_samples: usize) -> f32 {
        let read_pos = (self.write_pos + self.buffer_size - delay_samples) % self.buffer_size;
        self.buffers[channel][read_pos]
    }

    /// Get a slice of recent samples for analysis
    /// Returns samples from [write_pos - lookback_samples .. write_pos]
    pub fn get_recent_samples(&self, channel: usize, lookback_samples: usize) -> Vec<f32> {
        let lookback = lookback_samples.min(self.buffer_size);
        let mut result = Vec::with_capacity(lookback);

        for i in 0..lookback {
            let pos = (self.write_pos + self.buffer_size - lookback + i) % self.buffer_size;
            result.push(self.buffers[channel][pos]);
        }

        result
    }

    pub fn reset(&mut self) {
        for channel in &mut self.buffers {
            channel.fill(0.0);
        }
        self.write_pos = 0;
    }
}
