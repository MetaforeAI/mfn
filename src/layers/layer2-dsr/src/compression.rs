//! Real compression optimization for Layer 2 DSR
//! Implements actual bit-level compression on neural data

use std::collections::HashMap;
use anyhow::Result;

/// Real compression stats
#[derive(Debug)]
pub struct CompressionStats {
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub compression_ratio: f64,
    pub compression_time_us: u64,
}

/// Compresses neural spike data using run-length encoding
pub fn compress_spike_data(spikes: &[f32]) -> Result<(Vec<u8>, CompressionStats)> {
    let start_time = std::time::Instant::now();
    let original_bytes = spikes.len() * 4; // f32 = 4 bytes
    
    let mut compressed = Vec::new();
    let mut i = 0;
    
    while i < spikes.len() {
        let value = spikes[i];
        let mut count = 1u8;
        
        // Count consecutive identical values
        while i + (count as usize) < spikes.len() && 
              spikes[i + (count as usize)] == value && 
              count < 255 {
            count += 1;
        }
        
        // Store count + value
        compressed.push(count);
        compressed.extend_from_slice(&value.to_le_bytes());
        
        i += count as usize;
    }
    
    let compression_time = start_time.elapsed().as_micros() as u64;
    let stats = CompressionStats {
        original_bytes,
        compressed_bytes: compressed.len(),
        compression_ratio: original_bytes as f64 / compressed.len() as f64,
        compression_time_us: compression_time,
    };
    
    Ok((compressed, stats))
}

/// Decompresses run-length encoded spike data
pub fn decompress_spike_data(compressed: &[u8]) -> Result<Vec<f32>> {
    let mut spikes = Vec::new();
    let mut i = 0;
    
    while i < compressed.len() {
        if i + 4 >= compressed.len() {
            break;
        }
        
        let count = compressed[i];
        let value_bytes = &compressed[i+1..i+5];
        let value = f32::from_le_bytes([value_bytes[0], value_bytes[1], value_bytes[2], value_bytes[3]]);
        
        for _ in 0..count {
            spikes.push(value);
        }
        
        i += 5;
    }
    
    Ok(spikes)
}

/// Compresses neuron weights using quantization
pub fn compress_weights(weights: &[f32], bits: u8) -> Result<(Vec<u8>, CompressionStats)> {
    let start_time = std::time::Instant::now();
    let original_bytes = weights.len() * 4;
    
    // Find min/max for quantization range
    let min_weight = weights.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let max_weight = weights.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    
    let range = max_weight - min_weight;
    let levels = (1u32 << bits) - 1;
    let scale = levels as f32 / range;
    
    let mut compressed = Vec::new();
    
    // Store quantization parameters
    compressed.extend_from_slice(&min_weight.to_le_bytes());
    compressed.extend_from_slice(&max_weight.to_le_bytes());
    compressed.push(bits);
    
    // Quantize and pack weights
    match bits {
        8 => {
            for &weight in weights {
                let quantized = ((weight - min_weight) * scale) as u8;
                compressed.push(quantized);
            }
        },
        4 => {
            for chunk in weights.chunks(2) {
                let q1 = ((chunk[0] - min_weight) * scale) as u8 & 0xF;
                let q2 = if chunk.len() > 1 {
                    ((chunk[1] - min_weight) * scale) as u8 & 0xF
                } else { 0 };
                compressed.push((q2 << 4) | q1);
            }
        },
        _ => return Err(anyhow::anyhow!("Unsupported bit depth: {}", bits)),
    }
    
    let compression_time = start_time.elapsed().as_micros() as u64;
    let stats = CompressionStats {
        original_bytes,
        compressed_bytes: compressed.len(),
        compression_ratio: original_bytes as f64 / compressed.len() as f64,
        compression_time_us: compression_time,
    };
    
    Ok((compressed, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_spike_compression() {
        let spikes = vec![0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.5];
        let (compressed, stats) = compress_spike_data(&spikes).unwrap();
        let decompressed = decompress_spike_data(&compressed).unwrap();
        
        assert_eq!(spikes, decompressed);
        assert!(stats.compression_ratio > 1.0);
        println!("Spike compression: {:.2}x ratio", stats.compression_ratio);
    }
    
    #[test]
    fn test_weight_compression() {
        let weights: Vec<f32> = (0..100).map(|i| i as f32 * 0.01).collect();
        let (compressed, stats) = compress_weights(&weights, 8).unwrap();
        
        assert!(stats.compression_ratio > 1.0);
        println!("Weight compression: {:.2}x ratio in {}μs", 
                stats.compression_ratio, stats.compression_time_us);
    }
}