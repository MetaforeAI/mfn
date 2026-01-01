#!/usr/bin/env python3
"""
MFN Orchestrator HTTP Server
Exposes the MFN Orchestrator via HTTP API for integration with external systems
"""

from flask import Flask, request, jsonify
from flask_cors import CORS
from orchestrator import MFNOrchestrator, MemoryFlowResult
import logging
import requests
import time
from dataclasses import asdict

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

app = Flask(__name__)
CORS(app)  # Enable CORS for Node.js integration

# Global orchestrator instance
orchestrator = MFNOrchestrator()

@app.route('/health', methods=['GET'])
def health_check():
    """Health check endpoint"""
    return jsonify({
        'status': 'healthy',
        'service': 'mfn-orchestrator',
        'version': '1.0.0'
    })

@app.route('/memory/add', methods=['POST'])
def add_memory():
    """
    Add a memory through the orchestrator

    POST /memory/add
    Body: {
        "content": "message content",
        "tags": ["tag1", "tag2"],  # optional
        "context": ["context1"]     # optional
    }
    """
    try:
        data = request.get_json()

        if not data or 'content' not in data:
            return jsonify({'error': 'Missing content field'}), 400

        content = data['content']
        tags = data.get('tags', [])
        context = data.get('context', [])

        # Add timestamp to context for temporal sorting
        timestamp = int(time.time() * 1000)  # milliseconds

        # Add timestamp to context array for storage
        if not isinstance(context, list):
            context = []
        context_with_time = context + [f'timestamp:{timestamp}']

        logger.info(f"Adding memory with timestamp {timestamp}: {content[:50]}...")

        result = orchestrator.add_memory_flow(
            content=content,
            tags=tags,
            context=context_with_time
        )

        return jsonify({
            'success': result.success,
            'memory_id': result.memory_id,
            'total_time_ms': result.total_time_ms,
            'layer_results': result.layer_results,
            'decision': result.final_decision,
            'confidence': result.confidence
        })

    except Exception as e:
        logger.error(f"Error adding memory: {e}")
        return jsonify({'error': str(e)}), 500

@app.route('/memory/query', methods=['POST'])
def query_memory():
    """
    Query memories through the orchestrator

    POST /memory/query
    Body: {
        "query": "search query",
        "max_results": 5  # optional, default 5
    }
    """
    try:
        data = request.get_json()

        if not data or 'query' not in data:
            return jsonify({'error': 'Missing query field'}), 400

        query = data['query']
        max_results = data.get('max_results', 5)

        logger.info(f"Querying memory: {query}")

        result = orchestrator.query_memory_flow(
            query=query,
            max_results=max_results
        )

        # Extract actual results from layer responses
        memories = []

        # Try Layer 2 (similarity search) first
        if result.layer_results.get('layer2', {}).get('success'):
            l2_response = result.layer_results['layer2'].get('response', {})
            if isinstance(l2_response, dict) and 'results' in l2_response:
                memories = l2_response['results']

        # Fall back to Layer 3 if Layer 2 has no results
        if not memories and result.layer_results.get('layer3', {}).get('success'):
            l3_results = result.layer_results['layer3'].get('results', [])
            if l3_results:
                memories = l3_results

        # Try Layer 3 text search if other layers failed
        if not memories:
            try:
                logger.info("Trying Layer 3 text search")
                response = requests.post(
                    "http://localhost:8082/search/text",
                    json={"q": query, "limit": max_results},
                    timeout=5
                )

                if response.status_code == 200:
                    data = response.json()
                    results = data.get('results', [])
                    if results:
                        memories = results
                        logger.info(f"Layer 3 text search found {len(memories)} results")
            except Exception as e:
                logger.warning(f"Layer 3 text search failed: {e}")

        # Sort by timestamp (newest first)
        def extract_timestamp(memory):
            """Extract timestamp from memory created_at or context field"""
            # First try to use created_at from Layer 3
            created_at = memory.get('created_at')
            if created_at:
                try:
                    # Parse ISO format timestamp from Layer 3
                    from datetime import datetime
                    dt = datetime.fromisoformat(created_at.replace('Z', '+00:00'))
                    return dt.timestamp()
                except:
                    pass

            # Fall back to our custom timestamp in context
            context = memory.get('context', [])
            if isinstance(context, list):
                for item in context:
                    if isinstance(item, str) and item.startswith('timestamp:'):
                        try:
                            return int(item.split(':')[1]) / 1000.0  # Convert ms to seconds
                        except:
                            pass
            return 0  # No timestamp, put at end

        # Sort memories by timestamp descending (newest first)
        if memories:
            memories.sort(key=extract_timestamp, reverse=True)
            logger.info(f"Sorted {len(memories)} query results by recency")

        return jsonify({
            'success': result.success,
            'query': query,
            'total_time_ms': result.total_time_ms,
            'decision': result.final_decision,
            'confidence': result.confidence,
            'results': memories,
            'results_count': len(memories),
            'sorted_by': 'timestamp_desc',
            'layer_results': result.layer_results
        })

    except Exception as e:
        logger.error(f"Error querying memory: {e}")
        return jsonify({'error': str(e)}), 500

@app.route('/memory/context', methods=['POST'])
def get_context():
    """
    Get formatted context for AI prompts

    POST /memory/context
    Body: {
        "query": "current message",
        "max_results": 5,
        "format": "text"  # or "json"
    }
    """
    try:
        data = request.get_json()

        if not data or 'query' not in data:
            return jsonify({'error': 'Missing query field'}), 400

        query = data['query']
        max_results = data.get('max_results', 5)
        format_type = data.get('format', 'text')

        result = orchestrator.query_memory_flow(
            query=query,
            max_results=max_results
        )

        # Extract results
        memories = []
        if result.layer_results.get('layer2', {}).get('success'):
            l2_response = result.layer_results['layer2'].get('response', {})
            if isinstance(l2_response, dict) and 'results' in l2_response:
                memories = l2_response['results']

        if not memories and result.layer_results.get('layer3', {}).get('success'):
            l3_results = result.layer_results['layer3'].get('results', [])
            if l3_results:
                memories = l3_results

        # If no results from orchestrator flow, try Layer 3 directly
        if not memories:
            try:
                logger.info("No results from orchestrator, trying Layer 3 text search")
                # Use new text search endpoint for content-based matching
                response = requests.post(
                    "http://localhost:8082/search/text",
                    json={"q": query, "limit": max_results},
                    timeout=5
                )

                if response.status_code == 200:
                    search_response = response.json()
                    # Extract results from search response
                    search_results = search_response.get('results', [])
                    if search_results:
                        memories = search_results
                        logger.info(f"Retrieved {len(memories)} memories from Layer 3 text search")
            except Exception as e:
                logger.warning(f"Layer 3 search fallback failed: {e}")

        # If still no results, get most recent memories as context
        if not memories:
            try:
                logger.info("No semantic matches, retrieving recent memories")
                response = requests.get(
                    "http://localhost:8082/memories",
                    params={"limit": max_results},
                    timeout=5
                )

                if response.status_code == 200:
                    recent_data = response.json()
                    if 'memories' in recent_data:
                        memories = recent_data['memories']
                        logger.info(f"Returning {len(memories)} most recent memories as context")
            except Exception as e:
                logger.warning(f"Failed to get recent memories: {e}")

        # Sort by timestamp (newest first)
        def extract_timestamp(memory):
            """Extract timestamp from memory created_at or context field"""
            # First try to use created_at from Layer 3
            created_at = memory.get('created_at')
            if created_at:
                try:
                    # Parse ISO format timestamp from Layer 3
                    from datetime import datetime
                    dt = datetime.fromisoformat(created_at.replace('Z', '+00:00'))
                    return dt.timestamp()
                except:
                    pass

            # Fall back to our custom timestamp in context
            context = memory.get('context', [])
            if isinstance(context, list):
                for item in context:
                    if isinstance(item, str) and item.startswith('timestamp:'):
                        try:
                            return int(item.split(':')[1]) / 1000.0  # Convert ms to seconds
                        except:
                            pass
            return 0  # No timestamp, put at end

        # Sort memories by timestamp descending (newest first)
        if memories:
            memories.sort(key=extract_timestamp, reverse=True)
            logger.info(f"Sorted {len(memories)} memories by recency")

        if format_type == 'text':
            # Format as text for AI context
            if memories:
                formatted = "Relevant past memories (newest first):\n"
                for m in memories:
                    content = m.get('content', m.get('text', ''))
                    formatted += f"- {content}\n"
                context_text = formatted
            else:
                context_text = ""

            return jsonify({
                'context': context_text,
                'count': len(memories),
                'sorted_by': 'timestamp_desc',
                'query_time_ms': result.total_time_ms
            })
        else:
            # Return JSON
            return jsonify({
                'memories': memories,
                'count': len(memories),
                'sorted_by': 'timestamp_desc',
                'query_time_ms': result.total_time_ms
            })

    except Exception as e:
        logger.error(f"Error getting context: {e}")
        return jsonify({'error': str(e)}), 500

if __name__ == '__main__':
    logger.info("🧠 Starting MFN Orchestrator HTTP Server")
    logger.info("📡 Listening on http://localhost:11332")
    logger.info("🔗 Endpoints:")
    logger.info("   GET  /health")
    logger.info("   POST /memory/add")
    logger.info("   POST /memory/query")
    logger.info("   POST /memory/context")

    app.run(host='0.0.0.0', port=11332, debug=False)
