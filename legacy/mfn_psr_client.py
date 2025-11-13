"""
MFN Layer 5 (PSR) Python Client

Pattern Structure Registry client for APEX.
Connects to PSR socket service for pattern storage and retrieval.

Falls back to in-memory storage if PSR service is not available.
"""

import socket
import json
import struct
import uuid
import numpy as np
from typing import List, Dict, Optional, Tuple
from dataclasses import dataclass, asdict
import time


@dataclass
class PatternData:
    """Pattern data for PSR storage."""
    id: str
    name: str
    category: str  # temporal, spatial, transformational, relational
    embedding: List[float]  # 256-dim
    source_patterns: List[str] = None  # Parent patterns (for composition)
    slots: Dict = None
    constraints: List[Dict] = None
    composable_with: List[str] = None
    text_example: str = ""
    image_example: str = ""
    audio_example: str = ""
    code_example: str = ""
    activation_count: int = 0
    confidence: float = 1.0
    first_seen_step: int = 0
    last_used_step: int = 0

    def __post_init__(self):
        if self.source_patterns is None:
            self.source_patterns = []
        if self.slots is None:
            self.slots = {}
        if self.constraints is None:
            self.constraints = []
        if self.composable_with is None:
            self.composable_with = []


class PSRClient:
    """
    Client for MFN Layer 5 (Pattern Structure Registry).

    Handles connection to PSR socket service and provides
    pattern storage/retrieval operations.
    """

    def __init__(self, socket_path: str = "/tmp/mfn_layer5.sock", timeout: float = 1.0):
        self.socket_path = socket_path
        self.timeout = timeout
        self.connected = False
        self.sock = None

        # Fallback in-memory storage if PSR service unavailable
        self.fallback_storage: Dict[str, PatternData] = {}
        self.fallback_embeddings: Optional[np.ndarray] = None
        self.fallback_ids: List[str] = []

        # Try to connect
        self._connect()

    def _connect(self):
        """Attempt to connect to PSR socket."""
        try:
            self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.sock.settimeout(self.timeout)
            self.sock.connect(self.socket_path)
            self.connected = True
            print(f"✓ Connected to PSR service at {self.socket_path}")
        except (FileNotFoundError, ConnectionRefusedError, OSError) as e:
            self.connected = False
            print(f"⚠ PSR service not available ({e}). Using in-memory fallback.")

    def _send_request(self, request: Dict) -> Dict:
        """
        Send request to PSR service.

        Args:
            request: Request dictionary

        Returns:
            Response dictionary
        """
        if not self.connected:
            raise RuntimeError("Not connected to PSR service")

        # Serialize request
        request_json = json.dumps(request)
        request_bytes = request_json.encode('utf-8')

        # Send length prefix + message
        length = struct.pack('<I', len(request_bytes))
        self.sock.sendall(length + request_bytes)

        # Receive length prefix
        length_data = self.sock.recv(4)
        if not length_data:
            raise RuntimeError("Connection closed by PSR")

        response_length = struct.unpack('<I', length_data)[0]

        # Receive response
        response_bytes = b''
        while len(response_bytes) < response_length:
            chunk = self.sock.recv(response_length - len(response_bytes))
            if not chunk:
                raise RuntimeError("Connection closed while receiving response")
            response_bytes += chunk

        # Parse response
        response_json = response_bytes.decode('utf-8')
        response = json.loads(response_json)

        return response

    def store_pattern(self, pattern: PatternData) -> bool:
        """
        Store a pattern in PSR.

        Args:
            pattern: Pattern to store

        Returns:
            True if successful
        """
        if self.connected:
            try:
                request = {
                    "type": "store_pattern",
                    "request_id": str(uuid.uuid4()),
                    "pattern": asdict(pattern)
                }

                response = self._send_request(request)
                return response.get("success", False)

            except Exception as e:
                print(f"PSR store failed ({e}), using fallback")
                self.connected = False

        # Fallback: in-memory storage
        self.fallback_storage[pattern.id] = pattern

        # Update fallback embedding index
        self._update_fallback_index()

        return True

    def search_patterns(
        self,
        query_embedding: np.ndarray,
        top_k: int = 5,
        min_confidence: float = 0.0
    ) -> List[Tuple[str, float, PatternData]]:
        """
        Search for patterns similar to query embedding.

        Args:
            query_embedding: Query embedding (256-dim)
            top_k: Number of results to return
            min_confidence: Minimum confidence threshold

        Returns:
            List of (pattern_id, similarity, pattern_data)
        """
        if self.connected:
            try:
                request = {
                    "type": "search_patterns",
                    "request_id": str(uuid.uuid4()),
                    "query_embedding": query_embedding.tolist(),
                    "top_k": top_k,
                    "min_confidence": min_confidence
                }

                response = self._send_request(request)

                if response.get("success"):
                    results = []
                    for item in response.get("patterns", []):
                        pattern_dict = item["pattern"]
                        pattern = PatternData(**pattern_dict)
                        results.append((item["pattern_id"], item["similarity"], pattern))
                    return results

            except Exception as e:
                print(f"PSR search failed ({e}), using fallback")
                self.connected = False

        # Fallback: in-memory similarity search
        return self._fallback_search(query_embedding, top_k, min_confidence)

    def get_pattern(self, pattern_id: str) -> Optional[PatternData]:
        """
        Get a specific pattern by ID.

        Args:
            pattern_id: Pattern ID

        Returns:
            Pattern data or None
        """
        if self.connected:
            try:
                request = {
                    "type": "get_pattern",
                    "request_id": str(uuid.uuid4()),
                    "pattern_id": pattern_id
                }

                response = self._send_request(request)

                if response.get("success"):
                    pattern_dict = response["pattern"]
                    return PatternData(**pattern_dict)

            except Exception as e:
                print(f"PSR get failed ({e}), using fallback")
                self.connected = False

        # Fallback
        return self.fallback_storage.get(pattern_id)

    def list_patterns(
        self,
        category: Optional[str] = None,
        min_activation_count: int = 0,
        limit: int = 100,
        offset: int = 0
    ) -> List[PatternData]:
        """
        List patterns with optional filters.

        Args:
            category: Filter by category
            min_activation_count: Minimum activation count
            limit: Maximum results
            offset: Offset for pagination

        Returns:
            List of patterns
        """
        if self.connected:
            try:
                request = {
                    "type": "list_patterns",
                    "request_id": str(uuid.uuid4()),
                    "category": category,
                    "min_activation_count": min_activation_count,
                    "limit": limit,
                    "offset": offset
                }

                response = self._send_request(request)

                if response.get("success"):
                    patterns = []
                    for pattern_dict in response.get("patterns", []):
                        patterns.append(PatternData(**pattern_dict))
                    return patterns

            except Exception as e:
                print(f"PSR list failed ({e}), using fallback")
                self.connected = False

        # Fallback
        patterns = list(self.fallback_storage.values())

        # Apply filters
        if category:
            patterns = [p for p in patterns if p.category == category]

        if min_activation_count > 0:
            patterns = [p for p in patterns if p.activation_count >= min_activation_count]

        # Pagination
        return patterns[offset:offset + limit]

    def update_stats(
        self,
        pattern_id: str,
        activation_count_delta: int = 1,
        last_used_step: Optional[int] = None
    ) -> bool:
        """
        Update pattern usage statistics.

        Args:
            pattern_id: Pattern ID
            activation_count_delta: Change in activation count
            last_used_step: Last training step where pattern was used

        Returns:
            True if successful
        """
        if self.connected:
            try:
                request = {
                    "type": "update_stats",
                    "request_id": str(uuid.uuid4()),
                    "pattern_id": pattern_id,
                    "activation_count_delta": activation_count_delta,
                    "last_used_step": last_used_step
                }

                response = self._send_request(request)
                return response.get("success", False)

            except Exception as e:
                print(f"PSR update failed ({e}), using fallback")
                self.connected = False

        # Fallback
        if pattern_id in self.fallback_storage:
            pattern = self.fallback_storage[pattern_id]
            pattern.activation_count += activation_count_delta
            if last_used_step is not None:
                pattern.last_used_step = last_used_step
            return True

        return False

    def _update_fallback_index(self):
        """Update fallback embedding index for search."""
        if not self.fallback_storage:
            return

        # Build embedding matrix
        patterns = list(self.fallback_storage.values())
        embeddings = np.array([p.embedding for p in patterns], dtype=np.float32)

        self.fallback_embeddings = embeddings
        self.fallback_ids = [p.id for p in patterns]

    def _fallback_search(
        self,
        query_embedding: np.ndarray,
        top_k: int,
        min_confidence: float
    ) -> List[Tuple[str, float, PatternData]]:
        """In-memory similarity search fallback."""
        if self.fallback_embeddings is None or len(self.fallback_storage) == 0:
            return []

        # Normalize query
        query_norm = query_embedding / (np.linalg.norm(query_embedding) + 1e-8)

        # Normalize embeddings
        norms = np.linalg.norm(self.fallback_embeddings, axis=1, keepdims=True) + 1e-8
        embeddings_norm = self.fallback_embeddings / norms

        # Cosine similarity
        similarities = np.dot(embeddings_norm, query_norm)

        # Filter by confidence
        mask = similarities >= min_confidence
        filtered_similarities = similarities[mask]
        filtered_ids = [self.fallback_ids[i] for i, m in enumerate(mask) if m]

        # Top-k
        if len(filtered_similarities) == 0:
            return []

        top_k = min(top_k, len(filtered_similarities))
        top_indices = np.argsort(filtered_similarities)[-top_k:][::-1]

        results = []
        for idx in top_indices:
            pattern_id = filtered_ids[idx]
            similarity = float(filtered_similarities[idx])
            pattern = self.fallback_storage[pattern_id]
            results.append((pattern_id, similarity, pattern))

        return results

    def close(self):
        """Close connection to PSR."""
        if self.sock:
            self.sock.close()
            self.sock = None
        self.connected = False

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()


def test_psr_client():
    """Test PSR client (with fallback)."""
    print("Testing PSR Client...")

    client = PSRClient()

    # Test pattern storage
    pattern = PatternData(
        id="test_pattern_1",
        name="Test Sequence",
        category="temporal",
        embedding=np.random.randn(256).tolist(),
        text_example="A follows B",
        confidence=0.95
    )

    success = client.store_pattern(pattern)
    print(f"✓ Store pattern: {success}")

    # Test retrieval
    retrieved = client.get_pattern("test_pattern_1")
    print(f"✓ Get pattern: {retrieved is not None}")

    # Test search
    query = np.random.randn(256).astype(np.float32)
    results = client.search_patterns(query, top_k=5)
    print(f"✓ Search patterns: {len(results)} results")

    # Test list
    patterns = client.list_patterns()
    print(f"✓ List patterns: {len(patterns)} total")

    # Test stats update
    updated = client.update_stats("test_pattern_1", activation_count_delta=1)
    print(f"✓ Update stats: {updated}")

    client.close()
    print("\nPSR Client test complete!")


if __name__ == "__main__":
    test_psr_client()
