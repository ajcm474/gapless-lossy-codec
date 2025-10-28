//! Audio source implementation for rodio playback

/// Audio source for rodio that plays from a Vec<f32> of samples
pub struct SamplesSource
{
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    position: usize,
}

impl SamplesSource
{
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self
    {
        Self
        {
            samples,
            sample_rate,
            channels,
            position: 0,
        }
    }
}

impl Iterator for SamplesSource
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item>
    {
        if self.position < self.samples.len()
        {
            let sample = self.samples[self.position];
            self.position += 1;
            Some(sample)
        }
        else
        {
            None
        }
    }
}

impl rodio::Source for SamplesSource
{
    fn current_frame_len(&self) -> Option<usize>
    {
        None
    }

    fn channels(&self) -> u16
    {
        self.channels
    }

    fn sample_rate(&self) -> u32
    {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration>
    {
        None
    }
}