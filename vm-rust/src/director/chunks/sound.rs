use binary_reader::{BinaryReader, Endian};

use log::debug;
use wasm_bindgen::JsValue;
use web_sys::console;

use js_sys::Float32Array;

use crate::director::chunks::MediaChunk;

#[derive(Clone)]
pub struct SoundChunk {
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    sample_count: u32,
    codec: String,
    data: Vec<u8>,
pub version: u16,
}

impl SoundChunk {
    pub fn new(data: Vec<u8>) -> SoundChunk {
        SoundChunk {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_count: 0,
            codec: "raw_pcm".into(),
            data,
            version: 0,
        }
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn bits_per_sample(&self) -> u16 {
        self.bits_per_sample
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    pub fn codec(&self) -> String {
        self.codec.clone()
    }

    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }

    pub fn set_metadata(&mut self, sample_rate: u32, channels: u16, bits_per_sample: u16) {
        self.sample_rate = sample_rate;
        self.channels = channels;
        self.bits_per_sample = bits_per_sample;
        self.sample_count =
            (self.data.len() / (channels as usize * (bits_per_sample / 8) as usize)) as u32;
        console::log_1(
            &format!(
                "Updated metadata: channels={}, sample_rate={}, bits={}",
                self.channels, self.sample_rate, self.bits_per_sample
            )
            .into(),
        );
    }

    pub fn debug_get_samples(&self) -> Result<Float32Array, JsValue> {
        let max_samples = 100;
        let num_samples_to_process = std::cmp::min(self.sample_count as usize, max_samples);
        let num_output_floats = num_samples_to_process * self.channels as usize;

        // 1. Create the Float32Array to return to JS
        let output_array = Float32Array::new_with_length(num_output_floats as u32);

        // The data is Vec<u8> which we need to read as 16-bit integers
        let mut byte_reader = BinaryReader::from_vec(&self.data);
        // Director audio is Big Endian
        byte_reader.endian = Endian::Big;

        // 16-bit signed max value for normalization
        const MAX_I16_F: f32 = 32768.0;

        for i in 0..num_output_floats {
            // Read one 16-bit sample (u16) from the Big-Endian data.
            // BinaryReader handles the Big Endian interpretation for us.
            // Note: read_i16() would be better, but we need the normalization step.

            let signed_sample_i16 = match byte_reader.read_i16() {
                Ok(val) => val,
                Err(_) => break, // Stop if we run out of data
            };

            // 2. Normalization: Convert signed 16-bit integer to a float between -1.0 and 1.0
            let normalized_sample = signed_sample_i16 as f32 / MAX_I16_F;

            // 3. Write to the output array
            output_array.set_index(i as u32, normalized_sample);
        }

        console::log_1(&format!("Debug Sample Array size: {}", output_array.length()).into());
        Ok(output_array)
    }
}

impl Default for SoundChunk {
    fn default() -> Self {
        Self {
            channels: 1,
            sample_rate: 22050,
            bits_per_sample: 16,
            sample_count: 0,
            codec: "raw_pcm".to_string(),
            data: Vec::new(),
            version: 0,
        }
    }
}

impl SoundChunk {
    pub fn from_snd_chunk(reader: &mut BinaryReader, version: u16) -> Result<SoundChunk, String> {
        let original_endian = reader.endian;
        reader.endian = Endian::Big;

        let start_pos = reader.pos;

        // Read all bytes for reference
        let mut all_bytes = Vec::new();
        while let Ok(byte) = reader.read_u8() {
            all_bytes.push(byte);
        }
        reader.pos = start_pos;

        if all_bytes.len() < 10 {
            reader.endian = original_endian;
            return Err(format!("snd chunk too short: {} bytes", all_bytes.len()));
        }

        debug!("Parsing Mac snd resource ({} bytes)", all_bytes.len());

        // --- Parse Mac snd resource header ---
        // Format: type 1 (0x0001) or type 2 (0x0002)
        let format_type = reader.read_u16().map_err(|e| format!("Failed to read format type: {}", e))?;

        let num_commands: u16;
        match format_type {
            1 => {
                // Type 1: number of data types (modifiers), then modifiers, then commands
                let num_data_types = reader.read_u16().map_err(|e| format!("Type 1: {}", e))?;
                for _ in 0..num_data_types {
                    let _modifier_type = reader.read_u16().map_err(|e| format!("Modifier type: {}", e))?;
                    let _modifier_data = reader.read_u32().map_err(|e| format!("Modifier data: {}", e))?;
                }
                num_commands = reader.read_u16().map_err(|e| format!("Num commands: {}", e))?;
            }
            2 => {
                // Type 2: reference count, then commands
                let _ref_count = reader.read_u16().map_err(|e| format!("Ref count: {}", e))?;
                num_commands = reader.read_u16().map_err(|e| format!("Num commands: {}", e))?;
            }
            _ => {
                // Unknown format type - could be raw audio data or different format
                // Fall back: treat entire data as audio with default settings
                reader.endian = original_endian;
                debug!("Unknown snd format type 0x{:04X}, treating as raw audio", format_type);
                return Ok(SoundChunk {
                    channels: 1,
                    sample_rate: 22050,
                    bits_per_sample: 16,
                    sample_count: (all_bytes.len() / 2) as u32,
                    codec: "raw_pcm".to_string(),
                    data: all_bytes,
                    version,
                });
            }
        }

        // Read sound commands, look for bufferCmd (0x8051 or 0x0051)
        let mut sound_header_offset: Option<usize> = None;
        for _ in 0..num_commands {
            let cmd = reader.read_u16().map_err(|e| format!("Command: {}", e))?;
            let _param1 = reader.read_u16().map_err(|e| format!("Param1: {}", e))?;
            let param2 = reader.read_u32().map_err(|e| format!("Param2: {}", e))?;

            // bufferCmd = 0x0051, with data offset flag = 0x8051
            if (cmd & 0x7FFF) == 0x0051 {
                sound_header_offset = Some(param2 as usize);
            }
        }

        // Sound data header follows commands, or is at the offset specified by bufferCmd
        let header_pos = match sound_header_offset {
            Some(offset) => start_pos + offset,
            None => reader.pos, // Immediately after commands
        };
        reader.pos = header_pos;

        // --- Parse Sound Data Header ---
        let _sample_ptr = reader.read_u32().map_err(|e| format!("samplePtr: {}", e))?;
        let length_or_channels = reader.read_u32().map_err(|e| format!("length/channels: {}", e))?;
        let sample_rate_fixed = reader.read_u32().map_err(|e| format!("sampleRate: {}", e))?;
        let _loop_start = reader.read_u32().map_err(|e| format!("loopStart: {}", e))?;
        let _loop_end = reader.read_u32().map_err(|e| format!("loopEnd: {}", e))?;
        let encode = reader.read_u8().map_err(|e| format!("encode: {}", e))?;
        let _base_frequency = reader.read_u8().map_err(|e| format!("baseFrequency: {}", e))?;

        // Convert Fixed-point 16.16 sample rate to integer
        let sample_rate = sample_rate_fixed >> 16;

        let (channels, bits_per_sample, sample_count, audio_data_start);

        match encode {
            0x00 => {
                // Standard Sound Header (stdSH) - 8-bit unsigned mono
                // length_or_channels = numSamples
                channels = 1;
                bits_per_sample = 8;
                sample_count = length_or_channels;
                // Audio data starts immediately after the 22-byte header
                audio_data_start = (header_pos - start_pos) + 22;
                debug!(
                    "stdSH: {} Hz, 8-bit mono, {} samples, audio at offset {}",
                    sample_rate, sample_count, audio_data_start
                );
            }
            0xFF => {
                // Extended Sound Header (extSH) - can be 8 or 16 bit, mono or stereo
                // length_or_channels = numChannels
                channels = length_or_channels as u16;
                let num_frames = reader.read_u32().map_err(|e| format!("numFrames: {}", e))?;
                // Skip: AIFFSampleRate (10) + markerChunk (4) + instrumentChunks (4) + AESRecording (4)
                for _ in 0..22 {
                    let _ = reader.read_u8();
                }
                let sample_size = reader.read_u16().map_err(|e| format!("sampleSize: {}", e))?;
                bits_per_sample = if sample_size == 0 { 16 } else { sample_size };
                sample_count = num_frames;
                // Audio data starts at offset 64 from sound data header
                audio_data_start = (header_pos - start_pos) + 64;
                debug!(
                    "extSH: {} Hz, {}-bit, {} ch, {} frames, audio at offset {}",
                    sample_rate, bits_per_sample, channels, num_frames, audio_data_start
                );
            }
            0xFE => {
                // Compressed Sound Header (cmpSH)
                // Similar to extended header but with compression info
                channels = length_or_channels as u16;
                let num_frames = reader.read_u32().map_err(|e| format!("numFrames: {}", e))?;
                // Skip to get compression format info
                for _ in 0..22 {
                    let _ = reader.read_u8();
                }
                let sample_size = reader.read_u16().map_err(|e| format!("sampleSize: {}", e))?;
                bits_per_sample = if sample_size == 0 { 16 } else { sample_size };
                sample_count = num_frames;
                audio_data_start = (header_pos - start_pos) + 64;
                debug!(
                    "cmpSH: {} Hz, {}-bit, {} ch, {} frames, audio at offset {}",
                    sample_rate, bits_per_sample, channels, num_frames, audio_data_start
                );
            }
            _ => {
                // Unknown encode byte - default to 16-bit
                channels = 1;
                bits_per_sample = 16;
                sample_count = length_or_channels;
                audio_data_start = (header_pos - start_pos) + 22;
                debug!(
                    "Unknown encode 0x{:02X}: {} Hz, defaulting to 16-bit mono, audio at offset {}",
                    encode, sample_rate, audio_data_start
                );
            }
        }

        reader.endian = original_endian;

        // Extract only the audio data bytes (no snd resource header)
        let audio_data = if audio_data_start < all_bytes.len() {
            all_bytes[audio_data_start..].to_vec()
        } else {
            debug!("Warning: audio_data_start {} >= data length {}", audio_data_start, all_bytes.len());
            Vec::new()
        };

        if audio_data.is_empty() {
            return Err("snd chunk contains no audio data".to_string());
        }

        // Detect codec (MP3 vs PCM)
        let is_mp3 = audio_data.len() >= 2 && audio_data[0] == 0xFF && (audio_data[1] & 0xE0) == 0xE0;
        let codec = if is_mp3 { "mp3" } else { "raw_pcm" };

        let final_sample_count = if is_mp3 { 0 } else { sample_count };

        debug!(
            "Final snd: {} Hz, {}-bit, {} ch, codec={}, {} audio bytes, {} samples",
            sample_rate, bits_per_sample, channels, codec, audio_data.len(), final_sample_count
        );

        Ok(SoundChunk {
            channels,
            sample_rate,
            bits_per_sample,
            sample_count: final_sample_count,
            codec: codec.to_string(),
            data: audio_data,
            version,
        })
    }

    /// Convert to WAV bytes
    pub fn to_wav(&self) -> Vec<u8> {
        let mut wav = Vec::new();

        let byte_rate = self.sample_rate * self.channels as u32 * self.bits_per_sample as u32 / 8;
        let block_align = self.channels * self.bits_per_sample / 8;

        // RIFF header
        wav.extend_from_slice(b"RIFF");
        let chunk_size = 36 + self.data.len() as u32;
        wav.extend_from_slice(&chunk_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");

        // fmt subchunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&self.channels.to_le_bytes());
        wav.extend_from_slice(&self.sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&self.bits_per_sample.to_le_bytes());

        // data subchunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(self.data.len() as u32).to_le_bytes());

        // Audio data - Director stores 16-bit in big-endian, WAV needs little-endian
        if self.bits_per_sample == 16 {
            for chunk in self.data.chunks_exact(2) {
                wav.push(chunk[1]);
                wav.push(chunk[0]);
            }
            if self.data.len() % 2 == 1 {
                wav.push(*self.data.last().unwrap());
            }
        } else {
            wav.extend_from_slice(&self.data);
        }

        wav
    }

    pub fn from_media(media: &MediaChunk) -> SoundChunk {
        let codec = media.get_codec_name();

        // For IMA ADPCM, the data_size_field contains the uncompressed size
        // Calculate sample_count from uncompressed size, not compressed data
        let (sample_count, bits_per_sample) = if codec == "ima_adpcm" {
            // Director stores data_size_field as the number of SAMPLES, not bytes!
            // This is why we were getting half duration - we were dividing by 2
            let uncompressed_samples = media.data_size_field;
            (uncompressed_samples, 16)
        } else if codec == "mp3" {
            // For MP3, we can't easily calculate sample count without decoding
            // Use compressed size as estimate
            (0, 0)
        } else {
            // Raw PCM - data is in bytes, 16-bit = 2 bytes per sample
            ((media.audio_data.len() / 2) as u32, 16)
        };

        debug!(
            "from_media: codec={}, data_size_field={}, audio_data.len()={}, sample_count={}",
            codec,
            media.data_size_field,
            media.audio_data.len(),
            sample_count
        );

        SoundChunk {
            channels: 1,
            sample_rate: media.sample_rate,
            bits_per_sample,
            sample_count,
            codec: codec.to_string(),
            data: media.audio_data.clone(),
            version: 0,
        }
    }
}
