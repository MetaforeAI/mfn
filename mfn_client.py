"""
MFN Client - Protocol client for MFN services.

Layer 1 IFR Protocol (Zig - socket_server.zig):
  Newline-delimited JSON over Unix socket.
  Send: json_string + '\\n'
  Recv: json_string + '\\n'
  Message types: add_memory, query, ping, get_stats

Layer 2 DSR Protocol (Rust - socket_server.rs):
  4-byte length prefix (u32 little-endian) + JSON payload.
  Message types: AddMemory, SimilaritySearch, GetStats, Ping

Layer 3 ALM Protocol (Go - unix_socket_server.go):
  4-byte length prefix (u32 little-endian) + JSON payload.
  Message types: search, add_memory, add_association, get_stats, ping

Layer 4 CPE Protocol (Rust):
  4-byte length prefix (u32 little-endian) + JSON payload.
  Message types: AddMemoryContext, PredictContext, Ping, HealthCheck

Layer 5 PSR Protocol (Rust):
  4-byte length prefix (u32 little-endian) + JSON payload.
  Message types: AddPattern, SimilaritySearch, GetPattern, Ping, HealthCheck
"""

import socket
import struct
import json
import logging
import threading
import time
from collections import deque
from pathlib import Path
from typing import Optional, Dict, List, Any

logger = logging.getLogger(__name__)


class _ConnectionPool:
    """Bounded connection pool for a single MFN layer socket.

    Maintains up to ``max_size`` reusable connections.  Threads acquire a
    connection (blocking if all are in use), use it, then release it back.
    This keeps concurrent socket connections bounded regardless of how many
    Python threads are running.
    """

    def __init__(self, socket_path: str, max_size: int, timeout: float):
        self._socket_path = socket_path
        self._max_size = max_size
        self._timeout = timeout
        self._sem = threading.Semaphore(max_size)
        self._lock = threading.Lock()
        self._idle: deque[socket.socket] = deque()
        self._all: list[socket.socket] = []

    # -- public API --------------------------------------------------

    def acquire(self) -> socket.socket:
        """Get a connection (may block until one is available)."""
        self._sem.acquire()
        with self._lock:
            while self._idle:
                sock = self._idle.popleft()
                try:
                    sock.getpeername()
                    return sock
                except socket.error:
                    self._close_sock_unlocked(sock)
        # No idle connection — create a new one
        return self._new_connection()

    def release(self, sock: socket.socket) -> None:
        """Return a healthy connection to the pool."""
        with self._lock:
            self._idle.append(sock)
        self._sem.release()

    def discard(self, sock: socket.socket) -> None:
        """Dispose of a broken connection and flush stale idle connections.

        When one connection breaks (server restart, idle timeout), all idle
        connections in the pool are likely stale too.  Flush them so retries
        create fresh connections instead of grabbing another dead one.
        """
        # Collect sockets to close under the lock, close them after releasing
        to_close: list[socket.socket] = [sock]
        with self._lock:
            while self._idle:
                to_close.append(self._idle.popleft())
            # Remove all from tracking list
            for s in to_close:
                try:
                    self._all.remove(s)
                except ValueError:
                    pass
        # Close sockets outside the lock
        for s in to_close:
            try:
                s.close()
            except Exception:
                pass
        # Release semaphore slots: 1 for the discarded + 1 per flushed idle
        for _ in to_close:
            self._sem.release()

    def close_all(self) -> None:
        """Shut down every connection in the pool."""
        with self._lock:
            for s in self._all:
                try:
                    s.close()
                except Exception:
                    pass
            self._all.clear()
            self._idle.clear()

    # -- internals ---------------------------------------------------

    def _new_connection(self) -> socket.socket:
        if not Path(self._socket_path).exists():
            raise FileNotFoundError(
                f"Socket not found: {self._socket_path}")
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(self._timeout)
        sock.connect(self._socket_path)
        with self._lock:
            self._all.append(sock)
        return sock

    def _close_sock(self, sock: socket.socket) -> None:
        """Close a socket and remove from tracking. Caller must NOT hold self._lock."""
        try:
            sock.close()
        except Exception:
            pass
        with self._lock:
            try:
                self._all.remove(sock)
            except ValueError:
                pass

    def _close_sock_unlocked(self, sock: socket.socket) -> None:
        """Close a socket without touching self._lock. Caller must hold it."""
        try:
            sock.close()
        except Exception:
            pass
        try:
            self._all.remove(sock)
        except ValueError:
            pass


class MFNClient:
    """
    MFN client implementing protocol communication with all MFN layers.

    Manages persistent socket connections to MFN layers:
    - Layer 1 (IFR): Instant/Fast Retrieval (Zig) - newline-delimited JSON
    - Layer 2 (DSR): Dynamic Similarity Reservoir (Rust) - length-prefix + JSON
    - Layer 3 (ALM): Associative Link Memory (Go) - length-prefix + JSON
    - Layer 4 (CPE): Contextual Prediction Engine (Rust) - length-prefix + JSON
    - Layer 5 (PSR): Pattern Synthesis & Recognition (Rust) - length-prefix + JSON
    """

    # Per-layer connection pool sizes — tuned to each server's capacity
    _POOL_SIZES = {1: 20, 2: 20, 3: 20, 4: 20, 5: 20}

    def __init__(self, socket_base_path: str = "/tmp", timeout: float = 5.0):
        """
        Initialize MFN client.

        Args:
            socket_base_path: Base directory for Unix sockets (default: /tmp)
            timeout: Socket timeout in seconds (default: 5.0)
        """
        self.socket_base_path = socket_base_path
        self.timeout = timeout
        self._request_counter = 0
        self._counter_lock = threading.Lock()

        # Socket paths for each layer
        self.layer_sockets = {
            1: f"{socket_base_path}/mfn_test_layer1.sock",
            2: f"{socket_base_path}/mfn_test_layer2.sock",
            3: f"{socket_base_path}/mfn_test_layer3.sock",
            4: f"{socket_base_path}/mfn_test_layer4.sock",
            5: f"{socket_base_path}/mfn_test_layer5.sock",
        }

        # Bounded connection pools: each layer gets a pool of reusable
        # connections. Threads check out a connection, use it, return it.
        # This caps concurrent connections per layer (e.g. 40) regardless
        # of how many Python threads are running (e.g. 128).
        self._pools: Dict[int, _ConnectionPool] = {
            layer: _ConnectionPool(path, self._POOL_SIZES.get(layer, 40), timeout)
            for layer, path in self.layer_sockets.items()
        }

        logger.info(f"MFN Client initialized with sockets: {self.layer_sockets}")

    def _generate_request_id(self) -> str:
        """Generate unique request ID via thread-safe counter."""
        with self._counter_lock:
            self._request_counter += 1
            return str(self._request_counter)

    def _binary_request(self, layer: int, payload: dict, retries: int = 1) -> Optional[dict]:
        """
        Send a binary-protocol request and receive the response.

        Acquires a connection from the pool, uses it, then releases it.
        Auto-reconnects on BrokenPipeError/ConnectionError (up to `retries` times).

        Args:
            layer: MFN layer number (2-5)
            payload: Request payload dict
            retries: Number of reconnect attempts on connection errors

        Returns:
            Response dict or None on failure
        """
        pool = self._pools[layer]
        for attempt in range(1 + retries):
            try:
                sock = pool.acquire()
            except (FileNotFoundError, ConnectionError, OSError) as e:
                logger.error(f"L{layer} connect failed: {e}")
                return None
            try:
                self._send_binary_message(sock, payload)
                response = self._recv_binary_message(sock)
                if response is None:
                    pool.discard(sock)
                    if attempt < retries:
                        continue
                    return None
                pool.release(sock)
                return response
            except (BrokenPipeError, ConnectionError, ConnectionResetError, OSError) as e:
                logger.debug(f"L{layer} connection error (attempt {attempt+1}): {e}")
                pool.discard(sock)
                if attempt < retries:
                    continue
                logger.error(f"L{layer} request failed after {1+retries} attempts: {e}")
                return None
            except Exception as e:
                logger.error(f"L{layer} unexpected error: {e}")
                pool.discard(sock)
                return None
        return None

    def _send_binary_message(self, sock: socket.socket, payload: dict) -> None:
        """
        Send message using binary protocol: 4-byte length + JSON.

        Args:
            sock: Connected socket
            payload: Dictionary to send as JSON
        """
        # Serialize to JSON
        json_bytes = json.dumps(payload).encode('utf-8')

        # Send length prefix (4 bytes, little-endian u32)
        length = len(json_bytes)
        length_bytes = struct.pack('<I', length)

        sock.sendall(length_bytes + json_bytes)
        logger.debug(f"Sent message: type={payload.get('type')}, len={length}")

    def _drain_socket(self, sock: socket.socket) -> None:
        """Drain any remaining data from a socket to reset its state."""
        import select
        try:
            sock.setblocking(False)
            while True:
                ready = select.select([sock], [], [], 0)
                if not ready[0]:
                    break
                data = sock.recv(8192)
                if not data:
                    break
        except Exception:
            pass
        finally:
            try:
                sock.setblocking(True)
                sock.settimeout(self.timeout)
            except Exception:
                pass

    def _recv_binary_message(self, sock: socket.socket) -> Optional[dict]:
        """
        Receive message using binary protocol: 4-byte length + JSON.

        On invalid/corrupt data, drains the socket buffer to prevent
        cascading failures on subsequent calls.

        Args:
            sock: Connected socket

        Returns:
            Decoded JSON dictionary or None if failed
        """
        # Read 4-byte length prefix
        length_bytes = b''
        while len(length_bytes) < 4:
            chunk = sock.recv(4 - len(length_bytes))
            if not chunk:
                logger.error("Connection closed while reading length prefix")
                return None
            length_bytes += chunk

        # Decode length (little-endian u32)
        length = struct.unpack('<I', length_bytes)[0]

        # Sanity check — if invalid, the socket has stale/corrupt data
        if length == 0 or length > 10_000_000:
            logger.error(f"Invalid message length: {length} — draining socket")
            self._drain_socket(sock)
            return None

        # Read JSON payload
        json_bytes = b''
        while len(json_bytes) < length:
            chunk = sock.recv(min(length - len(json_bytes), 8192))
            if not chunk:
                logger.error("Connection closed while reading payload")
                return None
            json_bytes += chunk

        # Decode JSON
        try:
            payload = json.loads(json_bytes.decode('utf-8'))
            logger.debug(f"Received message: type={payload.get('type')}, len={length}")
            return payload
        except json.JSONDecodeError as e:
            logger.error(f"Failed to decode JSON: {e}")
            self._drain_socket(sock)
            return None

    def _send_ifr_message(self, sock: socket.socket, payload: dict) -> None:
        """
        Send message to Layer 1 IFR using newline-delimited JSON protocol.

        Protocol: JSON string + '\\n' over Unix socket.
        The Zig server parses JSON when it sees data starting with '{'.

        Args:
            sock: Connected socket
            payload: Dictionary to serialize as JSON (must include 'type' field)
        """
        json_line = json.dumps(payload, separators=(',', ':')) + '\n'
        try:
            sock.sendall(json_line.encode('utf-8'))
            logger.debug(f"IFR sent JSON: type={payload.get('type')}, len={len(json_line)}")
        except Exception as e:
            logger.error(f"IFR sendall failed: {e}")
            raise

    def _recv_ifr_message(self, sock: socket.socket) -> Optional[dict]:
        """
        Receive message from Layer 1 IFR using newline-delimited JSON protocol.

        Protocol: JSON string + '\\n' over Unix socket.
        Uses blocking recv — relies on socket timeout for deadline.

        Args:
            sock: Connected socket

        Returns:
            Decoded JSON dictionary or None if failed
        """
        import select

        buf = b''
        try:
            # Blocking read until we get a newline (complete JSON message)
            while b'\n' not in buf:
                chunk = sock.recv(4096)
                if not chunk:
                    logger.debug("IFR: connection closed")
                    return None
                buf += chunk
        except socket.timeout:
            if buf:
                logger.debug("IFR: timeout with %d partial bytes", len(buf))
            else:
                logger.debug("IFR: recv timeout, no data")
            return None
        except Exception as e:
            logger.debug("IFR: recv error: %s", e)
            return None

        # Parse all lines, return first valid non-spurious response
        for line in buf.strip().split(b'\n'):
            if not line:
                continue
            try:
                msg = json.loads(line.decode('utf-8'))
            except json.JSONDecodeError:
                continue
            # Skip spurious errors from Zig buffer bug
            if (msg.get('type') == 'error'
                    and msg.get('error') in ('Unknown protocol', 'Unknown request type')
                    and msg.get('request_id') == 'unknown'):
                logger.debug("IFR: filtering spurious error")
                continue
            logger.debug("IFR received: type=%s", msg.get('type'))
            # Drain any trailing data (spurious errors)
            try:
                sock.setblocking(False)
                while select.select([sock], [], [], 0)[0]:
                    if not sock.recv(4096):
                        break
            except Exception:
                pass
            finally:
                sock.setblocking(True)
                sock.settimeout(self.timeout)
            return msg

        logger.debug("IFR: no valid response in %d bytes", len(buf))
        return None

    def _ifr_request(self, payload: dict, retries: int = 1) -> Optional[dict]:
        """
        Send a newline-JSON request to L1 IFR and receive the response.

        Acquires a connection from the pool, uses it, releases it.
        Auto-reconnects on errors (up to `retries` times).

        Args:
            payload: Request payload dict (must include 'type' field)
            retries: Number of reconnect attempts on connection errors

        Returns:
            Response dict or None on failure
        """
        pool = self._pools[1]
        for attempt in range(1 + retries):
            try:
                sock = pool.acquire()
            except (FileNotFoundError, ConnectionError, OSError) as e:
                logger.warning(f"IFR connect failed: {e}")
                return None
            try:
                self._send_ifr_message(sock, payload)
                response = self._recv_ifr_message(sock)
                if response is None:
                    pool.discard(sock)
                    if attempt < retries:
                        continue
                    return None
                pool.release(sock)
                return response
            except (BrokenPipeError, ConnectionError, ConnectionResetError, OSError) as e:
                logger.debug(f"IFR connection error (attempt {attempt+1}): {e}")
                pool.discard(sock)
                if attempt < retries:
                    continue
                logger.error(f"IFR request failed after {1+retries} attempts: {e}")
                return None
            except Exception as e:
                logger.error(f"IFR unexpected error: {e}")
                pool.discard(sock)
                return None
        return None

    # ========================================================================
    # Layer 1 IFR Operations (Zig)
    # ========================================================================

    def ifr_add_memory(
        self,
        content: str,
        memory_data: str,
        pool_id: str = "crucible_training"
    ) -> bool:
        """
        Add exact-match memory to Layer 1 IFR.

        Args:
            content: Content to hash for exact matching
            memory_data: Memory data to store
            pool_id: Pool identifier for multi-pool support

        Returns:
            True if successful, False otherwise
        """
        response = self._ifr_request({
            "type": "add_memory",
            "request_id": self._generate_request_id(),
            "content": content,
            "memory_data": memory_data,
            "pool_id": pool_id,
        })
        if response and response.get("success"):
            return True
        error = response.get("error", "Unknown") if response else "No response"
        logger.error(f"IFR add_memory failed: {error}")
        return False

    def ifr_query(
        self,
        content: str,
        pool_id: str = "crucible_training"
    ) -> Optional[dict]:
        """
        Query Layer 1 IFR for exact match.

        Args:
            content: Content to query
            pool_id: Pool identifier for multi-pool support

        Returns:
            Result dictionary or None if not found
        """
        response = self._ifr_request({
            "type": "query",
            "request_id": self._generate_request_id(),
            "content": content,
            "pool_id": pool_id,
        })
        if response and response.get("found_exact"):
            return {
                "result": response.get("result"),
                "confidence": response.get("confidence", 1.0),
            }
        return None

    def ifr_ping(self) -> bool:
        """
        Ping Layer 1 IFR.

        Returns:
            True if responsive, False otherwise
        """
        response = self._ifr_request({
            "type": "ping",
            "request_id": self._generate_request_id(),
        })
        if response and response.get("type") == "pong":
            logger.info("IFR ping successful")
            return True
        return False

    def ifr_get_stats(self, pool_id: str = "crucible_training") -> Optional[dict]:
        """
        Get statistics from Layer 1 IFR.

        Args:
            pool_id: Pool identifier

        Returns:
            Statistics dictionary or None if failed
        """
        response = self._ifr_request({
            "type": "get_stats",
            "request_id": self._generate_request_id(),
            "pool_id": pool_id,
        })
        if response and response.get("type") == "stats_response":
            return response
        return None

    # ========================================================================
    # Layer 2 DSR Operations (Rust)
    # ========================================================================

    def dsr_add_memory(
        self,
        embedding: List[float],
        content: str,
        memory_id: Optional[int] = None,
        pool_id: str = "crucible_training",
        tags: Optional[List[str]] = None,
        metadata: Optional[Dict[str, str]] = None,
    ) -> bool:
        """
        Add memory to Layer 2 DSR.

        Args:
            embedding: Embedding vector (must match DSR embedding_dim, typically 512)
            content: Memory content string
            memory_id: Optional memory ID (auto-generated if None)
            pool_id: Pool identifier for multi-pool support
            tags: Optional list of tags
            metadata: Optional metadata dictionary

        Returns:
            True if successful, False otherwise
        """
        request_id = self._generate_request_id()
        if memory_id is None:
            memory_id = int(time.time() * 1000000)  # Microsecond timestamp

        payload = {
            "type": "AddMemory",
            "request_id": request_id,
            "pool_id": pool_id,
            "memory_id": memory_id,
            "embedding": embedding,
            "content": content,
            "tags": tags or [],
            "metadata": metadata or {},
        }

        response = self._binary_request(2, payload)

        if response:
            if response.get("type") == "Success":
                logger.debug(f"DSR add_memory succeeded: {response.get('data')}")
                return True
            elif response.get("type") == "Error":
                error = response.get("error", "Unknown error")
                logger.error(f"DSR add_memory failed: {error}")
                return False

        return False

    def dsr_similarity_search(
        self,
        query_embedding: List[float],
        top_k: int = 10,
        pool_id: str = "crucible_training",
        min_confidence: Optional[float] = None,
    ) -> Optional[List[dict]]:
        """
        Search Layer 2 DSR for similar memories.

        Args:
            query_embedding: Query embedding vector (must match DSR embedding_dim)
            top_k: Number of results to return
            pool_id: Pool identifier for multi-pool support
            min_confidence: Minimum confidence threshold

        Returns:
            List of match dictionaries or None if failed
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "SimilaritySearch",
            "request_id": request_id,
            "pool_id": pool_id,
            "query_embedding": query_embedding,
            "top_k": top_k,
        }
        if min_confidence is not None:
            payload["min_confidence"] = min_confidence

        response = self._binary_request(2, payload)

        if response:
            if response.get("type") == "Success":
                data = response.get("data", {})
                matches = data.get("matches", [])
                logger.debug(f"DSR search returned {len(matches)} matches")
                return matches
            elif response.get("type") == "Error":
                error = response.get("error", "Unknown error")
                logger.error(f"DSR search failed: {error}")
                return None

        return None

    def dsr_get_stats(self, pool_id: str = "crucible_training") -> Optional[dict]:
        """
        Get statistics from Layer 2 DSR.

        Args:
            pool_id: Pool identifier

        Returns:
            Statistics dictionary or None if failed
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "GetStats",
            "request_id": request_id,
            "pool_id": pool_id,
        }

        response = self._binary_request(2, payload)
        if response and response.get("type") == "Success":
            return response.get("data", {})
        return None

    def dsr_ping(self) -> bool:
        """
        Ping Layer 2 DSR.

        Returns:
            True if responsive, False otherwise
        """
        payload = {
            "type": "Ping",
            "request_id": self._generate_request_id(),
        }

        response = self._binary_request(2, payload)
        if response and response.get("type") == "Pong":
            logger.info("DSR ping successful")
            return True
        return False

    # ========================================================================
    # Layer 3 ALM Operations (Go)
    # ========================================================================

    def alm_add_memory(
        self,
        content: str,
        pool_id: str = "crucible_training",
        metadata: Optional[Dict[str, Any]] = None,
    ) -> bool:
        """
        Add memory to Layer 3 ALM.

        Args:
            content: Memory content string
            pool_id: Pool identifier for multi-pool support
            metadata: Optional metadata dictionary

        Returns:
            True if successful, False otherwise
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "add_memory",
            "request_id": request_id,
            "pool_id": pool_id,
            "content": content,
            "metadata": metadata or {},
        }

        response = self._binary_request(3, payload)
        if response and response.get("success"):
            logger.debug("ALM add_memory succeeded")
            return True
        return False

    def alm_search(
        self,
        query: str,
        limit: int = 10,
        pool_id: str = "crucible_training",
        min_confidence: Optional[float] = None,
    ) -> Optional[List[dict]]:
        """
        Search Layer 3 ALM.

        Args:
            query: Search query string
            limit: Maximum number of results
            pool_id: Pool identifier for multi-pool support
            min_confidence: Minimum confidence threshold

        Returns:
            List of search results or None if failed
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "search",
            "request_id": request_id,
            "pool_id": pool_id,
            "query": query,
            "limit": limit,
        }
        if min_confidence is not None:
            payload["min_confidence"] = min_confidence

        response = self._binary_request(3, payload)

        if response and response.get("success"):
            results = response.get("results", [])
            logger.debug(f"ALM search returned {len(results)} results")
            return results

        if response:
            error = response.get("error", "Unknown error")
            logger.debug(f"ALM search: {error}")
        return None

    def alm_get_stats(self, pool_id: str = "crucible_training") -> Optional[dict]:
        """
        Get statistics from Layer 3 ALM.

        Args:
            pool_id: Pool identifier

        Returns:
            Statistics dictionary or None if failed
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "get_stats",
            "request_id": request_id,
            "pool_id": pool_id,
        }

        response = self._binary_request(3, payload)
        if response and response.get("success"):
            return response.get("metadata", {})
        return None

    def alm_ping(self) -> bool:
        """
        Ping Layer 3 ALM.

        Returns:
            True if responsive, False otherwise
        """
        payload = {
            "type": "ping",
            "request_id": self._generate_request_id(),
        }

        response = self._binary_request(3, payload)
        if response and response.get("success"):
            logger.info("ALM ping successful")
            return True
        return False

    # ========================================================================
    # Layer 4 CPE Operations (Rust)
    # ========================================================================

    def cpe_add_context(
        self,
        memory_id: int,
        content: str,
        context: List[str],
        pool_id: str = "crucible_training"
    ) -> bool:
        """
        Add memory with context to Layer 4 CPE.

        Args:
            memory_id: Memory ID to associate context with
            content: Memory content string
            context: List of context strings (sequence)
            pool_id: Pool identifier for multi-pool support

        Returns:
            True if successful, False otherwise
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "AddMemoryContext",
            "request_id": request_id,
            "pool_id": pool_id,
            "memory_id": memory_id,
            "content": content,
            "context": context
        }

        response = self._binary_request(4, payload)
        if response and response.get("success"):
            logger.debug(f"CPE add_context succeeded: context_added={response.get('context_added')}")
            return True
        return False

    def cpe_predict(
        self,
        current_context: List[str],
        sequence_length: int = 5,
        pool_id: str = "crucible_training"
    ) -> Optional[List[dict]]:
        """
        Predict next memories based on current context.

        Args:
            current_context: Current context sequence
            sequence_length: Number of predictions to return
            pool_id: Pool identifier for multi-pool support

        Returns:
            List of prediction dictionaries or None if failed
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "PredictContext",
            "request_id": request_id,
            "pool_id": pool_id,
            "current_context": current_context,
            "sequence_length": sequence_length
        }

        response = self._binary_request(4, payload)
        if response and response.get("success"):
            predictions = response.get("predictions", [])
            logger.debug(f"CPE predict returned {len(predictions)} predictions")
            return predictions
        return None

    def cpe_ping(self) -> bool:
        """
        Ping Layer 4 CPE.

        Uses binary protocol (4-byte length prefix + JSON):
        Send: {"type":"Ping","request_id":"..."}
        Recv: {"type":"Pong","success":true,...}

        Returns:
            True if responsive, False otherwise
        """
        payload = {
            "type": "Ping",
            "request_id": self._generate_request_id(),
        }

        response = self._binary_request(4, payload)
        if response and response.get("type") == "Pong":
            logger.info("CPE ping successful")
            return True
        return False

    # ========================================================================
    # Layer 5 PSR Operations (Rust)
    # ========================================================================

    def psr_add_pattern(
        self,
        pattern_id: str,
        name: str,
        embedding: List[float],
        category: str = "Transformational",
        confidence: float = 0.95,
        text_example: str = "",
        source_patterns: Optional[List[str]] = None,
        pool_id: str = "crucible_training"
    ) -> bool:
        """
        Add pattern to Layer 5 PSR.

        Uses binary protocol (4-byte length prefix + JSON):
        Send: {"type":"AddPattern","request_id":"...","pattern":{...}}
        Recv: {"type":"AddPattern_Response","success":true,"pattern_id":"..."}

        Args:
            pattern_id: Unique pattern identifier string
            name: Human-readable pattern name
            embedding: 256-dimensional float embedding vector
            category: Pattern category (default: "Transformational")
            confidence: Pattern confidence score (default: 0.95)
            text_example: Optional text example for the pattern
            source_patterns: Optional list of source pattern IDs
            pool_id: Pool identifier for multi-pool support

        Returns:
            True if successful, False otherwise
        """
        request_id = self._generate_request_id()
        created_at = int(time.time() * 1000)

        payload = {
            "type": "AddPattern",
            "request_id": request_id,
            "pattern": {
                "id": pattern_id,
                "name": name,
                "category": category,
                "embedding": embedding,
                "source_patterns": source_patterns or [],
                "composable_with": [],
                "slots": {},
                "constraints": [],
                "domain": "Any",
                "codomain": "Any",
                "text_example": text_example,
                "image_example": "",
                "audio_example": "",
                "code_example": "",
                "activation_count": 0,
                "confidence": confidence,
                "first_seen_step": 0,
                "last_used_step": 0,
                "created_at": created_at,
            },
        }

        response = self._binary_request(5, payload)
        if response and response.get("type") == "AddPattern_Response":
            if response.get("success"):
                pid = response.get("pattern_id", pattern_id)
                logger.debug(f"PSR add_pattern succeeded: pattern_id={pid}")
                return True
        return False

    def psr_similarity_search(
        self,
        query_embedding: List[float],
        top_k: int = 5,
        min_confidence: float = 0.3,
        pool_id: str = "crucible_training"
    ) -> Optional[List[dict]]:
        """
        Search Layer 5 PSR for similar patterns by embedding.

        Uses binary protocol (4-byte length prefix + JSON):
        Send: {"type":"SimilaritySearch","embedding":[...],"top_k":N,"min_confidence":F}
        Recv: {"type":"SimilaritySearch_Response","success":true,"results":[...],"count":N}

        Args:
            query_embedding: Query embedding vector (256-dimensional)
            top_k: Number of results to return (default: 5)
            min_confidence: Minimum confidence threshold (default: 0.3)
            pool_id: Pool identifier for multi-pool support

        Returns:
            List of result dictionaries or None if failed
        """
        request_id = self._generate_request_id()

        payload = {
            "type": "SimilaritySearch",
            "request_id": request_id,
            "embedding": query_embedding,
            "top_k": top_k,
            "min_confidence": min_confidence,
        }

        response = self._binary_request(5, payload)
        if response and response.get("type") == "SimilaritySearch_Response":
            if response.get("success"):
                results = response.get("results", [])
                count = response.get("count", len(results))
                logger.debug(f"PSR similarity_search returned {count} results")
                return results
        return None

    def psr_synthesize(
        self,
        query_pattern: List[float],
        top_k: int = 5,
        pool_id: str = "crucible_training"
    ) -> Optional[List[dict]]:
        """
        Backward-compatible alias for psr_similarity_search.

        Args:
            query_pattern: Query embedding vector
            top_k: Number of results to return
            pool_id: Pool identifier for multi-pool support

        Returns:
            List of result dictionaries or None if failed
        """
        return self.psr_similarity_search(
            query_embedding=query_pattern,
            top_k=top_k,
            pool_id=pool_id,
        )

    def psr_ping(self) -> bool:
        """
        Ping Layer 5 PSR.

        Uses binary protocol (4-byte length prefix + JSON):
        Send: {"type":"Ping","request_id":"..."}
        Recv: {"type":"Pong","success":true,"layer":"Layer5_PSR","status":"operational"}

        Returns:
            True if responsive, False otherwise
        """
        payload = {
            "type": "Ping",
            "request_id": self._generate_request_id(),
        }

        response = self._binary_request(5, payload)
        if response and response.get("type") == "Pong":
            logger.info("PSR ping successful")
            return True
        return False

    def psr_get_stats(self) -> Optional[dict]:
        """
        Get statistics from Layer 5 PSR via HealthCheck.

        Uses binary protocol (4-byte length prefix + JSON):
        Send: {"type":"HealthCheck","request_id":"..."}
        Recv: {"type":"HealthCheck_Response","success":true,"status":"healthy","pattern_count":N}

        Returns:
            Statistics dictionary or None if failed
        """
        payload = {
            "type": "HealthCheck",
            "request_id": self._generate_request_id(),
        }

        response = self._binary_request(5, payload)
        if response and response.get("type") == "HealthCheck_Response":
            return response
        return None

    # ========================================================================
    # Generic Store/Retrieve API (for MFNAdamW optimizer)
    # ========================================================================

    def store(self, key: str, data: bytes, layer: int = 3) -> Dict[str, Any]:
        """
        Generic store method for optimizer state storage.

        Args:
            key: Key to store data under
            data: Binary data to store
            layer: MFN layer (default 3 for ALM)

        Returns:
            Response dictionary with 'success' field
        """
        if layer == 3:
            # Layer 3 ALM: Too large for JSON protocol (2+MB per state)
            # Store to filesystem directly instead
            import os
            import hashlib

            # Create optimizer states directory
            state_dir = "/usr/lib/alembic/mfn/memory/optimizer_states"
            os.makedirs(state_dir, exist_ok=True)

            # Use hash of key as filename to avoid filesystem issues
            key_hash = hashlib.sha256(key.encode()).hexdigest()[:16]
            state_file = os.path.join(state_dir, f"{key_hash}.pt")

            try:
                with open(state_file, 'wb') as f:
                    f.write(data)
                return {"success": True}
            except Exception as e:
                logger.error(f"Failed to write optimizer state to {state_file}: {e}")
                return {"success": False}
        else:
            logger.warning(f"Store for layer {layer} not implemented, only layer 3 (ALM) supported")
            return {"success": False}

    def retrieve(self, key: str, layer: int = 3) -> Optional[Dict[str, Any]]:
        """
        Generic retrieve method for optimizer state retrieval.

        Args:
            key: Key to retrieve data for
            layer: MFN layer (default 3 for ALM)

        Returns:
            Response dictionary with 'success' and 'data' fields, or None if not found
        """
        if layer == 3:
            # Layer 3 ALM: Read from filesystem
            import os
            import hashlib

            state_dir = "/usr/lib/alembic/mfn/memory/optimizer_states"
            key_hash = hashlib.sha256(key.encode()).hexdigest()[:16]
            state_file = os.path.join(state_dir, f"{key_hash}.pt")

            if os.path.exists(state_file):
                try:
                    with open(state_file, 'rb') as f:
                        data = f.read()
                    return {
                        "success": True,
                        "data": data
                    }
                except Exception as e:
                    logger.error(f"Failed to read optimizer state from {state_file}: {e}")
                    return None

            # Not found
            return None
        else:
            logger.warning(f"Retrieve for layer {layer} not implemented, only layer 3 (ALM) supported")
            return None

    # ========================================================================
    # Connection Management
    # ========================================================================

    def close(self):
        """Close all connection pools."""
        for pool in self._pools.values():
            pool.close_all()
        logger.info("All MFN connections closed")

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()

    def __del__(self):
        """Destructor to ensure connections are closed."""
        try:
            self.close()
        except Exception:
            pass


# Convenience function for testing
def test_connectivity():
    """Test connectivity to all MFN layers."""
    client = MFNClient()
    results = {}

    # Test Layer 1 IFR
    try:
        if client.ifr_ping():
            results[1] = "Connected"
        else:
            results[1] = "Not responding"
    except Exception as e:
        results[1] = f"Error: {e}"

    # Test Layer 2 DSR
    try:
        if client.dsr_ping():
            results[2] = "Connected"
        else:
            results[2] = "Not responding"
    except Exception as e:
        results[2] = f"Error: {e}"

    # Test Layer 3 ALM
    try:
        if client.alm_ping():
            results[3] = "Connected"
        else:
            results[3] = "Not responding"
    except Exception as e:
        results[3] = f"Error: {e}"

    # Test Layer 4 CPE
    try:
        if client.cpe_ping():
            results[4] = "Connected"
        else:
            results[4] = "Not responding"
    except Exception as e:
        results[4] = f"Error: {e}"

    # Test Layer 5 PSR
    try:
        if client.psr_ping():
            results[5] = "Connected"
        else:
            results[5] = "Not responding"
    except Exception as e:
        results[5] = f"Error: {e}"

    client.close()
    return results


if __name__ == "__main__":
    # Run connectivity test
    print("Testing MFN connectivity...")
    results = test_connectivity()
    for layer, status in results.items():
        print(f"  Layer {layer}: {status}")
