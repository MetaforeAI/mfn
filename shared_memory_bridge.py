#!/usr/bin/env python3
"""
Real shared memory bridge for MFN layers
Implements actual memory-mapped communication between layers
"""

import mmap
import struct
import threading
import time
from typing import Dict, Optional, List
from dataclasses import dataclass

@dataclass
class MemorySegment:
    """Real memory segment with actual data"""
    layer_id: int
    data: bytes
    timestamp: float
    size: int

class SharedMemoryBridge:
    """Real shared memory implementation using mmap"""
    
    def __init__(self, segment_size: int = 1024 * 1024):  # 1MB segments
        self.segment_size = segment_size
        self.segments: Dict[str, mmap.mmap] = {}
        self.locks: Dict[str, threading.Lock] = {}
        
        # Create shared segments for each layer
        self._create_layer_segments()
    
    def _create_layer_segments(self):
        """Create real memory-mapped segments"""
        layer_names = ["layer1_ifr", "layer2_dsr", "layer3_alm", "layer4_cpe"]
        
        for layer_name in layer_names:
            try:
                # Create memory-mapped file
                segment = mmap.mmap(-1, self.segment_size)
                self.segments[layer_name] = segment
                self.locks[layer_name] = threading.Lock()
                
                # Initialize header: [size:8][timestamp:8][data...]
                segment.seek(0)
                segment.write(struct.pack('QQ', 0, 0))  # Empty segment
                
                print(f"✅ Created shared segment for {layer_name}: {self.segment_size} bytes")
                
            except Exception as e:
                print(f"❌ Failed to create segment for {layer_name}: {e}")
    
    def write_data(self, layer_name: str, data: bytes) -> bool:
        """Write data to shared memory segment"""
        if layer_name not in self.segments:
            return False
        
        segment = self.segments[layer_name]
        lock = self.locks[layer_name]
        
        with lock:
            # Check if data fits (16 bytes header + data)
            if len(data) + 16 > self.segment_size:
                print(f"⚠️  Data too large for {layer_name}: {len(data)} bytes")
                return False
            
            try:
                # Write header: size + timestamp
                segment.seek(0)
                timestamp = time.time_ns()
                segment.write(struct.pack('QQ', len(data), timestamp))
                
                # Write data
                segment.write(data)
                
                return True
                
            except Exception as e:
                print(f"❌ Write failed for {layer_name}: {e}")
                return False
    
    def read_data(self, layer_name: str) -> Optional[MemorySegment]:
        """Read data from shared memory segment"""
        if layer_name not in self.segments:
            return None
        
        segment = self.segments[layer_name]
        lock = self.locks[layer_name]
        
        with lock:
            try:
                # Read header
                segment.seek(0)
                header = segment.read(16)
                if len(header) < 16:
                    return None
                
                data_size, timestamp = struct.unpack('QQ', header)
                
                if data_size == 0:
                    return None  # Empty segment
                
                # Read data
                data = segment.read(data_size)
                if len(data) != data_size:
                    return None
                
                return MemorySegment(
                    layer_id=hash(layer_name) & 0xFFFF,
                    data=data,
                    timestamp=timestamp / 1e9,  # Convert to seconds
                    size=data_size
                )
                
            except Exception as e:
                print(f"❌ Read failed for {layer_name}: {e}")
                return None
    
    def get_segment_stats(self) -> Dict[str, Dict]:
        """Get real statistics about memory segments"""
        stats = {}
        
        for layer_name, segment in self.segments.items():
            lock = self.locks[layer_name]
            
            with lock:
                try:
                    segment.seek(0)
                    header = segment.read(16)
                    data_size, timestamp = struct.unpack('QQ', header)
                    
                    stats[layer_name] = {
                        'segment_size': self.segment_size,
                        'data_size': data_size,
                        'utilization': data_size / self.segment_size,
                        'last_write_ns': timestamp,
                        'age_seconds': (time.time_ns() - timestamp) / 1e9 if timestamp > 0 else 0
                    }
                    
                except Exception as e:
                    stats[layer_name] = {'error': str(e)}
        
        return stats
    
    def cleanup(self):
        """Clean up shared memory segments"""
        for layer_name, segment in self.segments.items():
            try:
                segment.close()
                print(f"✅ Cleaned up {layer_name} segment")
            except Exception as e:
                print(f"⚠️  Cleanup failed for {layer_name}: {e}")

def test_shared_memory_performance():
    """Test real shared memory performance"""
    print("🔄 Testing Real Shared Memory Performance")
    print("=" * 45)
    
    bridge = SharedMemoryBridge()
    
    # Test data of various sizes
    test_sizes = [100, 1000, 10000, 100000]  # bytes
    
    for size in test_sizes:
        test_data = b"A" * size
        
        # Measure write performance
        start_time = time.perf_counter_ns()
        success = bridge.write_data("layer2_dsr", test_data)
        write_time_ns = time.perf_counter_ns() - start_time
        
        if not success:
            print(f"  {size:6} bytes: Write FAILED")
            continue
        
        # Measure read performance
        start_time = time.perf_counter_ns()
        segment = bridge.read_data("layer2_dsr")
        read_time_ns = time.perf_counter_ns() - start_time
        
        # Verify data integrity
        data_ok = segment and len(segment.data) == size
        
        print(f"  {size:6} bytes: W={write_time_ns/1000:6.1f}μs R={read_time_ns/1000:6.1f}μs {'✅' if data_ok else '❌'}")
    
    # Show segment statistics
    stats = bridge.get_segment_stats()
    print(f"\n📊 Memory Segment Statistics:")
    for layer, stat in stats.items():
        if 'error' not in stat:
            print(f"  {layer:<12}: {stat['utilization']*100:5.1f}% used, age {stat['age_seconds']:.1f}s")
    
    bridge.cleanup()
    return True

if __name__ == "__main__":
    test_shared_memory_performance()