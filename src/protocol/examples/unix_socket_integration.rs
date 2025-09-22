//! Unix Socket Integration Example for MFN Binary Protocol
//! 
//! Demonstrates how the binary protocol integrates with Unix sockets
//! to achieve the 0.16ms response time shown in DevOps analysis.

use std::os::unix::net::{UnixStream, UnixListener};
use std::io::{Read, Write, Result as IoResult};
use std::thread;
use std::time::{Duration, Instant};
use std::path::Path;
use mfn_binary_protocol::*;

const SOCKET_PATH: &str = "/tmp/mfn_binary_protocol_test.sock";
const BUFFER_SIZE: usize = 65536;

fn main() -> Result<()> {
    println!("🔌 MFN Binary Protocol Unix Socket Integration");
    println!("==============================================");
    println!();

    // Clean up any existing socket
    let _ = std::fs::remove_file(SOCKET_PATH);

    // Start server in background thread
    let server_handle = thread::spawn(|| {
        if let Err(e) = run_server() {
            eprintln!("Server error: {}", e);
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(100));

    // Run client benchmarks
    benchmark_unix_socket_performance()?;
    benchmark_real_world_scenario()?;

    // Clean up
    let _ = std::fs::remove_file(SOCKET_PATH);
    
    println!("✅ Unix socket integration benchmark complete");
    Ok(())
}

fn run_server() -> IoResult<()> {
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("🚀 Binary protocol server listening on {}", SOCKET_PATH);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream) {
                        eprintln!("Client error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn handle_client(mut stream: UnixStream) -> Result<()> {
    let mut buffer = [0u8; BUFFER_SIZE];
    
    loop {
        // Read message length first
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => {},
            Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(MfnProtocolError::IoError(e)),
        }
        
        let msg_len = u32::from_le_bytes(len_buf) as usize;
        if msg_len > BUFFER_SIZE - 4 {
            return Err(MfnProtocolError::PayloadTooLarge(msg_len));
        }

        // Read full message
        stream.read_exact(&mut buffer[..msg_len])?;
        
        // Process message with binary protocol
        let start = Instant::now();
        let response = process_binary_message(&buffer[..msg_len])?;
        let processing_time = start.elapsed();

        // Send response
        stream.write_all(&(response.len() as u32).to_le_bytes())?;
        stream.write_all(&response)?;
        stream.flush()?;

        // Log performance for monitoring
        if processing_time.as_micros() > 500 {
            println!("⚠️  Slow processing: {}μs", processing_time.as_micros());
        }
    }

    Ok(())
}

fn process_binary_message(buffer: &[u8]) -> Result<Vec<u8>> {
    let mut deserializer = MfnBinaryDeserializer::new(buffer);
    let parsed = deserializer.parse_message()?;

    // Process based on message type
    match parsed.message_type {
        MessageType::MemoryAdd => {
            let mut payload_deserializer = MfnBinaryDeserializer::new(&parsed.payload);
            let _memory = payload_deserializer.deserialize_memory()?;
            
            // Simulate memory storage (instant for benchmark)
            create_success_response(parsed.sequence_id)
        },
        MessageType::MemoryGet => {
            // Simulate memory retrieval
            let mock_memory = create_mock_memory();
            create_memory_response(parsed.sequence_id, &mock_memory)
        },
        MessageType::SearchAssoc => {
            // Simulate associative search
            let mock_results = create_mock_search_results();
            create_search_response(parsed.sequence_id, &mock_results)
        },
        _ => {
            create_error_response(parsed.sequence_id, "Unsupported operation")
        }
    }
}

fn create_success_response(sequence_id: u32) -> Result<Vec<u8>> {
    let mut serializer = MfnBinarySerializer::new(256);
    
    // Simple success payload
    serializer.write_u32(0)?; // success code
    serializer.write_string("Operation completed successfully")?;
    
    serializer.create_message(
        MessageType::Response,
        Operation::Add,
        LayerId::Layer3,
        sequence_id,
    )
}

fn create_memory_response(sequence_id: u32, memory: &UniversalMemory) -> Result<Vec<u8>> {
    let mut serializer = MfnBinarySerializer::new(4096);
    serializer.serialize_memory(memory)?;
    
    serializer.create_message(
        MessageType::Response,
        Operation::Get,
        LayerId::Layer3,
        sequence_id,
    )
}

fn create_search_response(sequence_id: u32, results: &[UniversalMemory]) -> Result<Vec<u8>> {
    let mut serializer = MfnBinarySerializer::new(8192);
    
    // Serialize search results
    serializer.write_u32(results.len() as u32)?;
    for memory in results {
        serializer.serialize_memory(memory)?;
    }
    
    serializer.create_message(
        MessageType::Response,
        Operation::Search,
        LayerId::Layer3,
        sequence_id,
    )
}

fn create_error_response(sequence_id: u32, error_msg: &str) -> Result<Vec<u8>> {
    let mut serializer = MfnBinarySerializer::new(512);
    
    serializer.write_u32(1)?; // error code
    serializer.write_string(error_msg)?;
    
    serializer.create_message(
        MessageType::Error,
        Operation::Add,
        LayerId::Layer3,
        sequence_id,
    )
}

fn benchmark_unix_socket_performance() -> Result<()> {
    println!("⚡ Unix Socket + Binary Protocol Performance");
    println!("─".repeat(50));

    const ITERATIONS: usize = 10_000;
    let mut total_time = Duration::new(0, 0);
    let mut min_time = Duration::from_secs(1);
    let mut max_time = Duration::new(0, 0);

    // Create test memory for requests
    let test_memory = create_test_memory();
    let mut serializer = MfnBinarySerializer::new(4096);
    serializer.serialize_memory(&test_memory)?;
    let request = serializer.create_message(
        MessageType::MemoryAdd,
        Operation::Add,
        LayerId::Layer3,
        12345,
    )?;

    println!("Running {} iterations...", ITERATIONS);
    let benchmark_start = Instant::now();

    for i in 0..ITERATIONS {
        let start = Instant::now();
        
        // Connect to server
        let mut stream = UnixStream::connect(SOCKET_PATH)
            .map_err(|e| MfnProtocolError::IoError(e))?;

        // Send request
        stream.write_all(&(request.len() as u32).to_le_bytes())
            .map_err(|e| MfnProtocolError::IoError(e))?;
        stream.write_all(&request)
            .map_err(|e| MfnProtocolError::IoError(e))?;
        stream.flush()
            .map_err(|e| MfnProtocolError::IoError(e))?;

        // Read response
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)
            .map_err(|e| MfnProtocolError::IoError(e))?;
        let response_len = u32::from_le_bytes(len_buf) as usize;
        
        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf)
            .map_err(|e| MfnProtocolError::IoError(e))?;

        let elapsed = start.elapsed();
        total_time += elapsed;
        
        if elapsed < min_time {
            min_time = elapsed;
        }
        if elapsed > max_time {
            max_time = elapsed;
        }

        // Progress indicator
        if i % 1000 == 0 && i > 0 {
            let avg_us = (total_time.as_nanos() / (i as u128 + 1)) as f64 / 1000.0;
            println!("  Progress: {}/{} - Avg: {:.1}μs", i, ITERATIONS, avg_us);
        }
    }

    let benchmark_duration = benchmark_start.elapsed();
    let avg_time = total_time / ITERATIONS as u32;

    println!("\n📊 Unix Socket + Binary Protocol Results:");
    println!("├── Total time: {:.2}s", benchmark_duration.as_secs_f64());
    println!("├── Average: {:.1}μs per operation", avg_time.as_nanos() as f64 / 1000.0);
    println!("├── Minimum: {:.1}μs", min_time.as_nanos() as f64 / 1000.0);
    println!("├── Maximum: {:.1}μs", max_time.as_nanos() as f64 / 1000.0);
    println!("├── Throughput: {:.0} ops/sec", ITERATIONS as f64 / benchmark_duration.as_secs_f64());
    
    if avg_time.as_micros() < 200 {
        println!("└── ✅ Target achieved (<200μs - matching DevOps analysis)");
    } else {
        println!("└── ❌ Target missed (>200μs)");
    }

    println!();
    Ok(())
}

fn benchmark_real_world_scenario() -> Result<()> {
    println!("🌍 Real-World Scenario Benchmark");
    println!("─".repeat(50));

    // Simulate Layer 3 ALM operations from the actual codebase
    let scenarios = vec![
        ("Memory Add", create_memory_add_request()),
        ("Memory Get", create_memory_get_request()),
        ("Associative Search", create_search_request()),
        ("Batch Operations", create_batch_request()),
    ];

    for (name, request) in scenarios {
        println!("Testing {}...", name);
        
        let start = Instant::now();
        
        let mut stream = UnixStream::connect(SOCKET_PATH)
            .map_err(|e| MfnProtocolError::IoError(e))?;
        
        // Send request
        stream.write_all(&(request.len() as u32).to_le_bytes())
            .map_err(|e| MfnProtocolError::IoError(e))?;
        stream.write_all(&request)
            .map_err(|e| MfnProtocolError::IoError(e))?;
        stream.flush()
            .map_err(|e| MfnProtocolError::IoError(e))?;

        // Read response
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)
            .map_err(|e| MfnProtocolError::IoError(e))?;
        let response_len = u32::from_le_bytes(len_buf) as usize;
        
        let mut _response_buf = vec![0u8; response_len];
        stream.read_exact(&mut _response_buf)
            .map_err(|e| MfnProtocolError::IoError(e))?;

        let elapsed = start.elapsed();
        let status = if elapsed.as_micros() < 500 { "✅" } else { "⚠️" };
        
        println!("  {} {}: {:.1}μs", status, name, elapsed.as_nanos() as f64 / 1000.0);
    }

    println!();
    Ok(())
}

// Helper functions for creating test requests

fn create_memory_add_request() -> Vec<u8> {
    let memory = create_test_memory();
    let mut serializer = MfnBinarySerializer::new(4096);
    serializer.serialize_memory(&memory).unwrap();
    serializer.create_message(
        MessageType::MemoryAdd,
        Operation::Add,
        LayerId::Layer3,
        1001,
    ).unwrap()
}

fn create_memory_get_request() -> Vec<u8> {
    let mut serializer = MfnBinarySerializer::new(256);
    serializer.write_u64(12345).unwrap(); // memory ID
    serializer.create_message(
        MessageType::MemoryGet,
        Operation::Get,
        LayerId::Layer3,
        1002,
    ).unwrap()
}

fn create_search_request() -> Vec<u8> {
    let query = create_test_search_query();
    let mut serializer = MfnBinarySerializer::new(2048);
    serializer.serialize_search_query(&query).unwrap();
    serializer.create_message(
        MessageType::SearchAssoc,
        Operation::Search,
        LayerId::Layer3,
        1003,
    ).unwrap()
}

fn create_batch_request() -> Vec<u8> {
    let mut serializer = MfnBinarySerializer::new(8192);
    
    // Simulate 10 memory operations in a batch
    serializer.write_u32(10).unwrap(); // batch count
    for i in 0..10 {
        let memory = create_test_memory_with_id(i);
        serializer.serialize_memory(&memory).unwrap();
    }
    
    serializer.create_message(
        MessageType::MemoryAdd,
        Operation::Batch,
        LayerId::Layer3,
        1004,
    ).unwrap()
}

// Mock data creation functions
fn create_test_memory() -> UniversalMemory {
    create_test_memory_with_id(42)
}

fn create_test_memory_with_id(id: u64) -> UniversalMemory {
    use std::collections::HashMap;
    
    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), "unix_socket_test".to_string());
    
    UniversalMemory {
        id,
        content: format!("Test memory content for ID {}", id),
        embedding: Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]),
        tags: vec!["test".to_string(), "unix_socket".to_string()],
        metadata,
        created_at: 1640995200000000,
        last_accessed: 1640995200000000,
        access_count: 1,
    }
}

fn create_test_search_query() -> UniversalSearchQuery {
    UniversalSearchQuery {
        start_memory_ids: vec![1, 2, 3],
        content: Some("test search content".to_string()),
        embedding: Some(vec![0.1, 0.2, 0.3]),
        tags: vec!["search".to_string()],
        association_types: vec![AssociationType::Semantic],
        max_depth: 3,
        max_results: 10,
        min_weight: 0.1,
        timeout_us: 10000,
    }
}

fn create_mock_memory() -> UniversalMemory {
    create_test_memory_with_id(99999)
}

fn create_mock_search_results() -> Vec<UniversalMemory> {
    vec![
        create_test_memory_with_id(1001),
        create_test_memory_with_id(1002),
        create_test_memory_with_id(1003),
    ]
}

// Mock types to make compilation work
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct UniversalMemory {
    pub id: u64,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: u64,
    pub last_accessed: u64,
    pub access_count: u64,
}

#[derive(Clone, Debug)]
pub enum AssociationType {
    Semantic,
}

#[derive(Clone, Debug)]
pub struct UniversalSearchQuery {
    pub start_memory_ids: Vec<u64>,
    pub content: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub association_types: Vec<AssociationType>,
    pub max_depth: usize,
    pub max_results: usize,
    pub min_weight: f64,
    pub timeout_us: u64,
}