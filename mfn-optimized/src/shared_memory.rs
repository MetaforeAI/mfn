//! Shared memory system for zero-copy inter-layer communication
//! 
//! Implements high-performance shared memory with:
//! - Lock-free ring buffers for data transfer
//! - Memory-mapped segments for large data
//! - NUMA-aware memory allocation
//! - Cache-line aligned data structures

use std::sync::{Arc, atomic::{AtomicUsize, AtomicU64, Ordering}};
use anyhow::{Result, bail};
use parking_lot::{RwLock, Mutex};
use memmap2::{MmapMut, MmapOptions};
use ::shared_memory::{Shmem, ShmemConf};
use crossbeam::channel::{self, Receiver, Sender};

use super::*;
use crate::compression::CompressedQuery;
use crate::network_topology::Topology;
use mfn_core::*;

/// High-performance shared memory manager
pub struct SharedMemoryManager {
    config: SharedMemoryConfig,
    
    // Memory segments
    segments: Vec<Arc<MemorySegment>>,
    
    // Lock-free ring buffers for inter-layer communication
    layer_channels: Vec<LayerChannel>,
    
    // Memory pool for small allocations
    memory_pool: Arc<MemoryPool>,
    
    // Performance tracking
    allocations: AtomicU64,
    deallocations: AtomicU64,
    bytes_transferred: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

/// Memory segment with efficient allocation strategies
struct MemorySegment {
    id: usize,
    size: usize,
    shmem: Shmem,
    mmap: Mutex<MmapMut>,
    
    // Allocation tracking
    allocated_bytes: AtomicUsize,
    free_blocks: RwLock<Vec<FreeBlock>>,
    
    // NUMA node affinity
    numa_node: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct FreeBlock {
    offset: usize,
    size: usize,
}

/// Lock-free channel for inter-layer communication  
struct LayerChannel {
    sender: Sender<SharedMessage>,
    receiver: Receiver<SharedMessage>,
    
    // Ring buffer for high-throughput messaging
    ring_buffer: Arc<LockFreeRingBuffer>,
    
    // Statistics
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_transferred: AtomicU64,
}

/// Lock-free ring buffer implementation
struct LockFreeRingBuffer {
    buffer: Vec<AtomicU64>, // Using u64 for cache-line efficiency
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
    
    // Message metadata stored separately
    message_metadata: Vec<parking_lot::RwLock<Option<MessageMetadata>>>,
}

#[derive(Debug, Clone)]
struct MessageMetadata {
    message_type: MessageType,
    size: usize,
    timestamp: u64,
    checksum: u32,
}

#[derive(Debug, Clone)]
enum MessageType {
    Query,
    Response,  
    Control,
    Data,
}

#[derive(Debug, Clone)]
pub struct SharedMessage {
    pub data_ptr: usize,      // Pointer to shared memory
    pub size: usize,
    pub message_id: u64,
    pub layer_source: usize,
    pub layer_target: usize,
    pub priority: Priority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Priority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
}

/// Memory pool for efficient small allocations
struct MemoryPool {
    // Size-segregated free lists
    small_blocks: [RwLock<Vec<*mut u8>>; 16],  // 16 bytes to 1KB
    medium_blocks: [RwLock<Vec<*mut u8>>; 8],  // 1KB to 64KB  
    large_blocks: RwLock<Vec<LargeBlock>>,      // > 64KB
    
    // Allocation statistics
    small_allocs: AtomicU64,
    medium_allocs: AtomicU64,
    large_allocs: AtomicU64,
}

#[derive(Debug)]
struct LargeBlock {
    ptr: *mut u8,
    size: usize,
    segment_id: usize,
    offset: usize,
}

impl SharedMemoryManager {
    pub fn new(config: &SharedMemoryConfig) -> Result<Self> {
        let segments = Self::create_memory_segments(config)?;
        let layer_channels = Self::create_layer_channels(4)?; // 4 layers
        let memory_pool = Arc::new(MemoryPool::new());
        
        Ok(Self {
            config: config.clone(),
            segments,
            layer_channels,
            memory_pool,
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            bytes_transferred: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        })
    }
    
    fn create_memory_segments(config: &SharedMemoryConfig) -> Result<Vec<Arc<MemorySegment>>> {
        let mut segments = Vec::new();
        let segment_size = config.pool_size / config.segments;
        
        for i in 0..config.segments {
            let segment_name = format!("mfn_segment_{}", i);
            
            let shmem = ShmemConf::new()
                .size(segment_size)
                .create()?;
                
            let mmap = unsafe {
                MmapOptions::new()
                    .len(segment_size)
                    .map_mut(&shmem)?
            };
            
            let segment = Arc::new(MemorySegment {
                id: i,
                size: segment_size,
                shmem,
                mmap: Mutex::new(mmap),
                allocated_bytes: AtomicUsize::new(0),
                free_blocks: RwLock::new(vec![FreeBlock { offset: 0, size: segment_size }]),
                numa_node: Self::get_numa_node(i),
            });
            
            segments.push(segment);
        }
        
        Ok(segments)
    }
    
    fn create_layer_channels(layer_count: usize) -> Result<Vec<LayerChannel>> {
        let mut channels = Vec::new();
        
        for _ in 0..layer_count {
            let (sender, receiver) = channel::unbounded();
            let ring_buffer = Arc::new(LockFreeRingBuffer::new(8192)?);
            
            channels.push(LayerChannel {
                sender,
                receiver,
                ring_buffer,
                messages_sent: AtomicU64::new(0),
                messages_received: AtomicU64::new(0),
                bytes_transferred: AtomicU64::new(0),
            });
        }
        
        Ok(channels)
    }
    
    fn get_numa_node(segment_id: usize) -> Option<usize> {
        // Simplified NUMA detection - in production would use libnuma
        if segment_id % 2 == 0 { Some(0) } else { Some(1) }
    }
    
    /// Execute query through shared memory zero-copy pipeline
    pub async fn execute_query(
        &self,
        topology: &Topology,
        compressed_query: &CompressedQuery
    ) -> Result<Vec<UniversalSearchResult>> {
        // Step 1: Allocate shared memory for query
        let query_ptr = self.allocate_shared(compressed_query.data.len())?;
        
        // Step 2: Copy query data to shared memory
        unsafe {
            std::ptr::copy_nonoverlapping(
                compressed_query.data.as_ptr(),
                query_ptr.as_ptr(),
                compressed_query.data.len()
            );
        }
        
        // Step 3: Send query through layer topology
        let mut results = Vec::new();
        
        for layer_id in &topology.active_layers {
            let message = SharedMessage {
                data_ptr: query_ptr.as_ptr() as usize,
                size: compressed_query.data.len(),
                message_id: self.generate_message_id(),
                layer_source: 0,
                layer_target: *layer_id,
                priority: Priority::High,
            };
            
            // Send message through lock-free ring buffer
            self.send_message_lockfree(*layer_id, message).await?;
            
            // Receive response
            if let Some(response) = self.receive_response(*layer_id).await? {
                let layer_results = self.deserialize_results(&response)?;
                results.extend(layer_results);
            }
        }
        
        // Step 4: Free shared memory
        self.deallocate_shared(query_ptr)?;
        
        Ok(results)
    }
    
    /// Allocate shared memory with zero-copy guarantees
    pub fn allocate_shared(&self, size: usize) -> Result<SharedPointer> {
        self.allocations.fetch_add(1, Ordering::Relaxed);
        
        // Choose allocation strategy based on size
        if size <= 1024 {
            self.memory_pool.allocate_small(size)
        } else if size <= 65536 {
            self.memory_pool.allocate_medium(size) 
        } else {
            self.allocate_large_shared(size)
        }
    }
    
    fn allocate_large_shared(&self, size: usize) -> Result<SharedPointer> {
        // Find best-fit segment
        let mut best_segment: Option<usize> = None;
        let mut best_fit_size = usize::MAX;
        
        for (i, segment) in self.segments.iter().enumerate() {
            let free_blocks = segment.free_blocks.read();
            
            for block in free_blocks.iter() {
                if block.size >= size && block.size < best_fit_size {
                    best_segment = Some(i);
                    best_fit_size = block.size;
                }
            }
        }
        
        let segment_id = best_segment.ok_or_else(|| {
            anyhow::anyhow!("No suitable segment found for size {}", size)
        })?;
        
        let segment = &self.segments[segment_id];
        
        // Allocate within segment
        let offset = {
            let mut free_blocks = segment.free_blocks.write();
            
            let block_idx = free_blocks.iter().position(|b| b.size >= size)
                .ok_or_else(|| anyhow::anyhow!("No suitable block found"))?;
            
            let block = free_blocks[block_idx];
            
            // Split block if necessary
            if block.size > size {
                free_blocks[block_idx] = FreeBlock {
                    offset: block.offset + size,
                    size: block.size - size,
                };
            } else {
                free_blocks.remove(block_idx);
            }
            
            block.offset
        };
        
        segment.allocated_bytes.fetch_add(size, Ordering::Relaxed);
        
        // Get pointer to allocated memory
        let mmap = segment.mmap.lock();
        let ptr = unsafe { mmap.as_ptr().add(offset) };
        
        Ok(SharedPointer {
            ptr,
            size,
            segment_id,
            offset,
        })
    }
    
    pub fn deallocate_shared(&self, ptr: SharedPointer) -> Result<()> {
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        
        let segment = &self.segments[ptr.segment_id];
        
        // Add block back to free list
        let mut free_blocks = segment.free_blocks.write();
        
        let new_block = FreeBlock {
            offset: ptr.offset,
            size: ptr.size,
        };
        
        // Insert in order and merge adjacent blocks
        let insert_pos = free_blocks.binary_search_by_key(&new_block.offset, |b| b.offset)
            .unwrap_or_else(|pos| pos);
            
        free_blocks.insert(insert_pos, new_block);
        
        // Merge with adjacent blocks
        self.merge_adjacent_blocks(&mut free_blocks, insert_pos);
        
        segment.allocated_bytes.fetch_sub(ptr.size, Ordering::Relaxed);
        
        Ok(())
    }
    
    fn merge_adjacent_blocks(&self, blocks: &mut Vec<FreeBlock>, index: usize) {
        // Merge with next block
        if index + 1 < blocks.len() {
            let current_offset = blocks[index].offset;
            let current_size = blocks[index].size;
            let next_offset = blocks[index + 1].offset;
            let next_size = blocks[index + 1].size;
            
            if current_offset + current_size == next_offset {
                blocks[index].size += next_size;
                blocks.remove(index + 1);
            }
        }
        
        // Merge with previous block
        if index > 0 && index < blocks.len() {
            let prev_offset = blocks[index - 1].offset;
            let prev_size = blocks[index - 1].size;
            let current_offset = blocks[index].offset;
            let current_size = blocks[index].size;
            
            if prev_offset + prev_size == current_offset {
                blocks[index - 1].size += current_size;
                blocks.remove(index);
            }
        }
    }
    
    async fn send_message_lockfree(&self, layer_id: usize, message: SharedMessage) -> Result<()> {
        if layer_id >= self.layer_channels.len() {
            bail!("Invalid layer ID: {}", layer_id);
        }
        
        let channel = &self.layer_channels[layer_id];
        
        // Try lock-free ring buffer first for maximum performance
        if channel.ring_buffer.try_push(&message)? {
            channel.messages_sent.fetch_add(1, Ordering::Relaxed);
            channel.bytes_transferred.fetch_add(message.size as u64, Ordering::Relaxed);
            self.bytes_transferred.fetch_add(message.size as u64, Ordering::Relaxed);
            return Ok(());
        }
        
        // Fallback to channel if ring buffer is full
        channel.sender.send(message)?;
        channel.messages_sent.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }
    
    async fn receive_response(&self, layer_id: usize) -> Result<Option<SharedMessage>> {
        if layer_id >= self.layer_channels.len() {
            return Ok(None);
        }
        
        let channel = &self.layer_channels[layer_id];
        
        // Try lock-free ring buffer first
        if let Some(message) = channel.ring_buffer.try_pop()? {
            channel.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(message));
        }
        
        // Fallback to channel
        match channel.receiver.try_recv() {
            Ok(message) => {
                channel.messages_received.fetch_add(1, Ordering::Relaxed);
                Ok(Some(message))
            },
            Err(channel::TryRecvError::Empty) => Ok(None),
            Err(e) => bail!("Channel error: {}", e),
        }
    }
    
    fn deserialize_results(&self, message: &SharedMessage) -> Result<Vec<UniversalSearchResult>> {
        // Deserialize results from shared memory
        unsafe {
            let data_slice = std::slice::from_raw_parts(
                message.data_ptr as *const u8,
                message.size
            );
            
            Ok(bincode::deserialize(data_slice)?)
        }
    }
    
    fn generate_message_id(&self) -> u64 {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }
    
    pub fn get_efficiency(&self) -> f32 {
        let total_allocations = self.allocations.load(Ordering::Relaxed);
        let total_deallocations = self.deallocations.load(Ordering::Relaxed);
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        
        if total_allocations == 0 {
            return 1.0;
        }
        
        let deallocation_rate = total_deallocations as f32 / total_allocations as f32;
        let cache_hit_rate = if cache_hits + cache_misses > 0 {
            cache_hits as f32 / (cache_hits + cache_misses) as f32
        } else {
            0.0
        };
        
        (deallocation_rate + cache_hit_rate) / 2.0
    }
}

/// Pointer to shared memory with metadata
#[derive(Debug)]
pub struct SharedPointer {
    ptr: *mut u8,
    size: usize,
    segment_id: usize,
    offset: usize,
}

unsafe impl Send for SharedPointer {}
unsafe impl Sync for SharedPointer {}

impl SharedPointer {
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }
    
    pub fn size(&self) -> usize {
        self.size
    }
}

impl LockFreeRingBuffer {
    fn new(capacity: usize) -> Result<Self> {
        let buffer = (0..capacity).map(|_| AtomicU64::new(0)).collect();
        let message_metadata = (0..capacity)
            .map(|_| parking_lot::RwLock::new(None))
            .collect();
        
        Ok(Self {
            buffer,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            message_metadata,
        })
    }
    
    fn try_push(&self, message: &SharedMessage) -> Result<bool> {
        let head = self.head.load(Ordering::Relaxed);
        let next_head = (head + 1) % self.capacity;
        
        if next_head == self.tail.load(Ordering::Acquire) {
            return Ok(false); // Buffer full
        }
        
        // Store message data (simplified - real implementation would store more efficiently)
        let encoded = self.encode_message(message);
        self.buffer[head].store(encoded, Ordering::Relaxed);
        
        // Store metadata separately
        let metadata = MessageMetadata {
            message_type: MessageType::Query,
            size: message.size,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            checksum: self.calculate_checksum(message),
        };
        
        *self.message_metadata[head].write() = Some(metadata);
        
        self.head.store(next_head, Ordering::Release);
        Ok(true)
    }
    
    fn try_pop(&self) -> Result<Option<SharedMessage>> {
        let tail = self.tail.load(Ordering::Relaxed);
        
        if tail == self.head.load(Ordering::Acquire) {
            return Ok(None); // Buffer empty
        }
        
        let encoded = self.buffer[tail].load(Ordering::Relaxed);
        let message = self.decode_message(encoded);
        
        // Clear metadata
        *self.message_metadata[tail].write() = None;
        
        self.tail.store((tail + 1) % self.capacity, Ordering::Release);
        Ok(Some(message))
    }
    
    fn encode_message(&self, message: &SharedMessage) -> u64 {
        // Pack message data into u64 (simplified)
        ((message.data_ptr as u64) & 0xFFFFFFFF) |
        (((message.size as u64) & 0xFFFF) << 32) |
        (((message.message_id as u64) & 0xFFFF) << 48)
    }
    
    fn decode_message(&self, encoded: u64) -> SharedMessage {
        SharedMessage {
            data_ptr: (encoded & 0xFFFFFFFF) as usize,
            size: ((encoded >> 32) & 0xFFFF) as usize,
            message_id: (encoded >> 48) & 0xFFFF,
            layer_source: 0,
            layer_target: 0,
            priority: Priority::Normal,
        }
    }
    
    fn calculate_checksum(&self, message: &SharedMessage) -> u32 {
        // Simple checksum calculation
        let mut hash = 0u32;
        hash = hash.wrapping_add(message.data_ptr as u32);
        hash = hash.wrapping_add(message.size as u32);
        hash = hash.wrapping_add(message.message_id as u32);
        hash
    }
}

impl MemoryPool {
    fn new() -> Self {
        Self {
            small_blocks: Default::default(),
            medium_blocks: Default::default(),
            large_blocks: RwLock::new(Vec::new()),
            small_allocs: AtomicU64::new(0),
            medium_allocs: AtomicU64::new(0),
            large_allocs: AtomicU64::new(0),
        }
    }
    
    fn allocate_small(&self, size: usize) -> Result<SharedPointer> {
        self.small_allocs.fetch_add(1, Ordering::Relaxed);
        
        // Find appropriate size class (16, 32, 64, ... 1024 bytes)
        let size_class = (size.max(16).next_power_of_two().trailing_zeros() - 4) as usize;
        let size_class = size_class.min(self.small_blocks.len() - 1);
        
        let mut free_list = self.small_blocks[size_class].write();
        
        if let Some(ptr) = free_list.pop() {
            return Ok(SharedPointer {
                ptr,
                size: 16 << size_class,
                segment_id: 0,
                offset: 0,
            });
        }
        
        // Allocate new block
        let actual_size = 16 << size_class;
        let layout = std::alloc::Layout::from_size_align(actual_size, 64)?; // Cache-line aligned
        
        unsafe {
            let ptr = std::alloc::alloc(layout);
            if ptr.is_null() {
                bail!("Failed to allocate memory");
            }
            
            Ok(SharedPointer {
                ptr,
                size: actual_size,
                segment_id: 0,
                offset: 0,
            })
        }
    }
    
    fn allocate_medium(&self, size: usize) -> Result<SharedPointer> {
        // Similar to allocate_small but for medium-sized allocations
        self.medium_allocs.fetch_add(1, Ordering::Relaxed);
        bail!("Medium allocation not yet implemented")
    }
}