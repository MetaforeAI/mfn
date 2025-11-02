// Test to verify orchestrator validation works
use mfn_core::{MfnOrchestrator, UniversalMemory, UniversalSearchQuery};

#[tokio::test]
async fn test_empty_orchestrator_rejects_add_memory() {
    let mut orchestrator = MfnOrchestrator::new();

    let memory = UniversalMemory::new(1, "test".to_string());

    let result = orchestrator.add_memory(memory).await;

    assert!(result.is_err(), "Expected error when adding memory to empty orchestrator");
    assert!(result.unwrap_err().to_string().contains("no layers registered"));
}

#[tokio::test]
async fn test_empty_orchestrator_rejects_search() {
    let mut orchestrator = MfnOrchestrator::new();

    let query = UniversalSearchQuery::default();

    let result = orchestrator.search(query).await;

    assert!(result.is_err(), "Expected error when searching with empty orchestrator");
    assert!(result.unwrap_err().to_string().contains("no layers registered"));
}
