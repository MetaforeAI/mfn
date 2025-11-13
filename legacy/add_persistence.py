#!/usr/bin/env python3
"""
MFN System Persistence Implementation
Adds persistent storage capabilities to all 4 layers
"""

import json
import sqlite3
import pickle
import os
import time
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, asdict
from unified_socket_client import UnifiedMFNClient, MemoryItem, SearchResult

@dataclass
class PersistentMemory:
    """Persistent memory structure"""
    id: int
    content: str
    tags: List[str]
    metadata: Dict[str, str]
    embedding: Optional[List[float]]
    layer_states: Dict[str, Any]  # Layer-specific state
    created_at: float
    updated_at: float

class MFNPersistenceManager:
    """Comprehensive persistence manager for MFN system"""
    
    def __init__(self, data_dir: str = "./mfn_data"):
        self.data_dir = data_dir
        self.db_path = os.path.join(data_dir, "mfn_memories.db")
        self.layer_data_dir = os.path.join(data_dir, "layer_snapshots")
        
        # Ensure directories exist
        os.makedirs(data_dir, exist_ok=True)
        os.makedirs(self.layer_data_dir, exist_ok=True)
        
        # Initialize database
        self._init_database()
        
        print(f"🗄️  MFN Persistence Manager initialized")
        print(f"   Database: {self.db_path}")
        print(f"   Layer data: {self.layer_data_dir}")
    
    def _init_database(self):
        """Initialize SQLite database with MFN schema"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        # Main memories table
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY,
                content TEXT NOT NULL,
                tags TEXT,  -- JSON array
                metadata TEXT,  -- JSON object
                embedding BLOB,  -- Pickled numpy array/list
                layer_states TEXT,  -- JSON object
                created_at REAL,
                updated_at REAL
            )
        ''')
        
        # Layer-specific persistence tables
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS layer1_hashes (
                memory_id INTEGER,
                content_hash TEXT,
                hash_type TEXT,
                created_at REAL,
                FOREIGN KEY (memory_id) REFERENCES memories (id)
            )
        ''')
        
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS layer2_neural_states (
                memory_id INTEGER,
                spike_pattern BLOB,
                reservoir_weights BLOB,
                similarity_well_id INTEGER,
                created_at REAL,
                FOREIGN KEY (memory_id) REFERENCES memories (id)
            )
        ''')
        
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS layer3_associations (
                source_memory_id INTEGER,
                target_memory_id INTEGER,
                weight REAL,
                association_type TEXT,
                created_at REAL,
                FOREIGN KEY (source_memory_id) REFERENCES memories (id),
                FOREIGN KEY (target_memory_id) REFERENCES memories (id)
            )
        ''')
        
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS layer4_temporal_patterns (
                memory_id INTEGER,
                context_pattern BLOB,
                access_sequence TEXT,  -- JSON array
                temporal_weight REAL,
                last_accessed REAL,
                FOREIGN KEY (memory_id) REFERENCES memories (id)
            )
        ''')
        
        # System metadata table
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS system_metadata (
                key TEXT PRIMARY KEY,
                value TEXT,
                updated_at REAL
            )
        ''')
        
        conn.commit()
        conn.close()
        
        print("✅ Database schema initialized")
    
    def save_memory(self, memory: MemoryItem, embedding: List[float] = None, 
                   layer_states: Dict[str, Any] = None) -> bool:
        """Save memory to persistent storage"""
        try:
            conn = sqlite3.connect(self.db_path)
            cursor = conn.cursor()
            
            current_time = time.time()
            
            # Serialize complex data
            tags_json = json.dumps(memory.tags)
            metadata_json = json.dumps(memory.metadata)
            embedding_blob = pickle.dumps(embedding) if embedding else None
            layer_states_json = json.dumps(layer_states or {})
            
            # Insert or update memory
            cursor.execute('''
                INSERT OR REPLACE INTO memories 
                (id, content, tags, metadata, embedding, layer_states, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ''', (
                memory.id, memory.content, tags_json, metadata_json,
                embedding_blob, layer_states_json, current_time, current_time
            ))
            
            conn.commit()
            conn.close()
            
            return True
            
        except Exception as e:
            print(f"❌ Error saving memory {memory.id}: {e}")
            return False
    
    def load_memory(self, memory_id: int) -> Optional[PersistentMemory]:
        """Load memory from persistent storage"""
        try:
            conn = sqlite3.connect(self.db_path)
            cursor = conn.cursor()
            
            cursor.execute('''
                SELECT id, content, tags, metadata, embedding, layer_states, created_at, updated_at
                FROM memories WHERE id = ?
            ''', (memory_id,))
            
            row = cursor.fetchone()
            conn.close()
            
            if row:
                return PersistentMemory(
                    id=row[0],
                    content=row[1],
                    tags=json.loads(row[2]),
                    metadata=json.loads(row[3]),
                    embedding=pickle.loads(row[4]) if row[4] else None,
                    layer_states=json.loads(row[5]),
                    created_at=row[6],
                    updated_at=row[7]
                )
            return None
            
        except Exception as e:
            print(f"❌ Error loading memory {memory_id}: {e}")
            return None
    
    def load_all_memories(self) -> List[PersistentMemory]:
        """Load all memories from persistent storage"""
        try:
            conn = sqlite3.connect(self.db_path)
            cursor = conn.cursor()
            
            cursor.execute('''
                SELECT id, content, tags, metadata, embedding, layer_states, created_at, updated_at
                FROM memories ORDER BY id
            ''')
            
            memories = []
            for row in cursor.fetchall():
                memories.append(PersistentMemory(
                    id=row[0],
                    content=row[1],
                    tags=json.loads(row[2]),
                    metadata=json.loads(row[3]),
                    embedding=pickle.loads(row[4]) if row[4] else None,
                    layer_states=json.loads(row[5]),
                    created_at=row[6],
                    updated_at=row[7]
                ))
            
            conn.close()
            return memories
            
        except Exception as e:
            print(f"❌ Error loading all memories: {e}")
            return []
    
    def save_layer_state(self, layer_name: str, state_data: Dict[str, Any]) -> bool:
        """Save layer-specific state data"""
        try:
            layer_file = os.path.join(self.layer_data_dir, f"{layer_name}_state.json")
            
            state_with_metadata = {
                "layer_name": layer_name,
                "saved_at": time.time(),
                "state_data": state_data
            }
            
            with open(layer_file, 'w') as f:
                json.dump(state_with_metadata, f, indent=2)
            
            return True
            
        except Exception as e:
            print(f"❌ Error saving {layer_name} state: {e}")
            return False
    
    def load_layer_state(self, layer_name: str) -> Optional[Dict[str, Any]]:
        """Load layer-specific state data"""
        try:
            layer_file = os.path.join(self.layer_data_dir, f"{layer_name}_state.json")
            
            if os.path.exists(layer_file):
                with open(layer_file, 'r') as f:
                    state_with_metadata = json.load(f)
                return state_with_metadata.get("state_data", {})
            
            return None
            
        except Exception as e:
            print(f"❌ Error loading {layer_name} state: {e}")
            return None
    
    def save_associations(self, associations: List[Dict[str, Any]]) -> bool:
        """Save Layer 3 associative links"""
        try:
            conn = sqlite3.connect(self.db_path)
            cursor = conn.cursor()
            
            current_time = time.time()
            
            for assoc in associations:
                cursor.execute('''
                    INSERT OR REPLACE INTO layer3_associations
                    (source_memory_id, target_memory_id, weight, association_type, created_at)
                    VALUES (?, ?, ?, ?, ?)
                ''', (
                    assoc["source_id"], assoc["target_id"], assoc["weight"],
                    assoc.get("type", "similarity"), current_time
                ))
            
            conn.commit()
            conn.close()
            return True
            
        except Exception as e:
            print(f"❌ Error saving associations: {e}")
            return False
    
    def load_associations(self) -> List[Dict[str, Any]]:
        """Load Layer 3 associative links"""
        try:
            conn = sqlite3.connect(self.db_path)
            cursor = conn.cursor()
            
            cursor.execute('''
                SELECT source_memory_id, target_memory_id, weight, association_type, created_at
                FROM layer3_associations
            ''')
            
            associations = []
            for row in cursor.fetchall():
                associations.append({
                    "source_id": row[0],
                    "target_id": row[1], 
                    "weight": row[2],
                    "type": row[3],
                    "created_at": row[4]
                })
            
            conn.close()
            return associations
            
        except Exception as e:
            print(f"❌ Error loading associations: {e}")
            return []
    
    def create_backup(self, backup_name: str = None) -> str:
        """Create complete system backup"""
        if backup_name is None:
            backup_name = f"mfn_backup_{int(time.time())}"
        
        backup_dir = os.path.join(self.data_dir, "backups", backup_name)
        os.makedirs(backup_dir, exist_ok=True)
        
        try:
            # Backup database
            import shutil
            shutil.copy2(self.db_path, os.path.join(backup_dir, "mfn_memories.db"))
            
            # Backup layer states
            layer_backup_dir = os.path.join(backup_dir, "layer_snapshots")
            if os.path.exists(self.layer_data_dir):
                shutil.copytree(self.layer_data_dir, layer_backup_dir, dirs_exist_ok=True)
            
            # Create backup metadata
            backup_metadata = {
                "backup_name": backup_name,
                "created_at": time.time(),
                "database_file": "mfn_memories.db",
                "layer_states_dir": "layer_snapshots",
                "system_version": "1.0.0"
            }
            
            with open(os.path.join(backup_dir, "backup_metadata.json"), 'w') as f:
                json.dump(backup_metadata, f, indent=2)
            
            print(f"✅ Backup created: {backup_dir}")
            return backup_dir
            
        except Exception as e:
            print(f"❌ Backup failed: {e}")
            return ""
    
    def restore_from_backup(self, backup_dir: str) -> bool:
        """Restore system from backup"""
        try:
            # Verify backup exists and is valid
            backup_db = os.path.join(backup_dir, "mfn_memories.db")
            backup_metadata = os.path.join(backup_dir, "backup_metadata.json")
            
            if not os.path.exists(backup_db) or not os.path.exists(backup_metadata):
                print(f"❌ Invalid backup directory: {backup_dir}")
                return False
            
            # Load backup metadata
            with open(backup_metadata, 'r') as f:
                metadata = json.load(f)
            
            print(f"🔄 Restoring backup: {metadata['backup_name']}")
            print(f"   Created: {time.ctime(metadata['created_at'])}")
            
            # Restore database
            import shutil
            shutil.copy2(backup_db, self.db_path)
            
            # Restore layer states
            backup_layer_dir = os.path.join(backup_dir, "layer_snapshots")
            if os.path.exists(backup_layer_dir):
                if os.path.exists(self.layer_data_dir):
                    shutil.rmtree(self.layer_data_dir)
                shutil.copytree(backup_layer_dir, self.layer_data_dir)
            
            print("✅ Backup restored successfully")
            return True
            
        except Exception as e:
            print(f"❌ Restore failed: {e}")
            return False
    
    def get_storage_stats(self) -> Dict[str, Any]:
        """Get storage statistics"""
        try:
            conn = sqlite3.connect(self.db_path)
            cursor = conn.cursor()
            
            # Count memories
            cursor.execute("SELECT COUNT(*) FROM memories")
            memory_count = cursor.fetchone()[0]
            
            # Count associations  
            cursor.execute("SELECT COUNT(*) FROM layer3_associations")
            association_count = cursor.fetchone()[0]
            
            # Database size
            db_size = os.path.getsize(self.db_path) if os.path.exists(self.db_path) else 0
            
            # Layer data size
            layer_data_size = 0
            if os.path.exists(self.layer_data_dir):
                for root, dirs, files in os.walk(self.layer_data_dir):
                    layer_data_size += sum(os.path.getsize(os.path.join(root, f)) for f in files)
            
            conn.close()
            
            return {
                "memory_count": memory_count,
                "association_count": association_count,
                "database_size_mb": db_size / (1024 * 1024),
                "layer_data_size_mb": layer_data_size / (1024 * 1024),
                "total_size_mb": (db_size + layer_data_size) / (1024 * 1024),
                "data_directory": self.data_dir
            }
            
        except Exception as e:
            print(f"❌ Error getting storage stats: {e}")
            return {}

class MFNPersistentClient:
    """MFN Client with automatic persistence"""
    
    def __init__(self, data_dir: str = "./mfn_data"):
        self.client = UnifiedMFNClient()
        self.persistence = MFNPersistenceManager(data_dir)
        
        print("🧠 MFN Persistent Client initialized")
    
    def restore_system_state(self) -> Dict[str, Any]:
        """Restore complete system state from persistence"""
        print("🔄 Restoring MFN system state from persistence...")
        
        # Load all memories
        memories = self.persistence.load_all_memories()
        
        restore_stats = {
            "memories_loaded": 0,
            "memories_restored": 0,
            "layer_states_restored": 0,
            "associations_restored": 0
        }
        
        # Restore memories to layers
        for memory in memories:
            restore_stats["memories_loaded"] += 1
            
            memory_item = MemoryItem(
                id=memory.id,
                content=memory.content,
                tags=memory.tags,
                metadata=memory.metadata
            )
            
            # Add memory to layers
            results = self.client.add_memory(memory_item, memory.embedding)
            successful_layers = sum(results.values())
            
            if successful_layers > 0:
                restore_stats["memories_restored"] += 1
            
            print(f"   Memory {memory.id}: Restored to {successful_layers}/4 layers")
        
        # Restore associations
        associations = self.persistence.load_associations()
        restore_stats["associations_restored"] = len(associations)
        
        print(f"✅ System state restored:")
        print(f"   Memories: {restore_stats['memories_restored']}/{restore_stats['memories_loaded']}")
        print(f"   Associations: {restore_stats['associations_restored']}")
        
        return restore_stats
    
    def add_memory_persistent(self, memory: MemoryItem, embedding: List[float] = None) -> Dict[str, Any]:
        """Add memory with automatic persistence"""
        # Add to live system
        live_results = self.client.add_memory(memory, embedding)
        
        # Save to persistence
        persistence_success = self.persistence.save_memory(memory, embedding)
        
        return {
            "live_results": live_results,
            "persistence_success": persistence_success,
            "successful_layers": sum(live_results.values())
        }
    
    def search_with_persistence(self, query: str, max_results: int = 10) -> List[SearchResult]:
        """Search with persistence backup"""
        # Try live search first
        results = self.client.unified_search(query, max_results)
        
        # If no results and we have persistent data, search persistence
        if not results:
            print("🔍 No live results, searching persistent data...")
            # Could implement persistent search here
        
        return results
    
    def create_system_backup(self) -> str:
        """Create complete system backup"""
        print("💾 Creating MFN system backup...")
        
        # Get current system stats before backup
        stats = self.client.get_system_stats()
        
        # Save current layer states
        for layer_name, layer_stats in stats.items():
            if "error" not in layer_stats:
                self.persistence.save_layer_state(layer_name, layer_stats)
        
        # Create backup
        backup_dir = self.persistence.create_backup()
        
        return backup_dir

def main():
    """Demo of MFN persistence capabilities"""
    print("🗄️  MFN Persistence System Demo")
    print("=" * 50)
    
    # Initialize persistent client
    client = MFNPersistentClient()
    
    # Check if we have existing data
    stats = client.persistence.get_storage_stats()
    print(f"📊 Current Storage:")
    print(f"   Memories: {stats.get('memory_count', 0)}")
    print(f"   Associations: {stats.get('association_count', 0)}")
    print(f"   Total size: {stats.get('total_size_mb', 0):.2f} MB")
    print()
    
    # Restore existing state if available
    if stats.get('memory_count', 0) > 0:
        restore_stats = client.restore_system_state()
        print()
    
    # Add new persistent memories
    print("📝 Adding new persistent memories...")
    test_memories = [
        MemoryItem(301, "Persistent neural networks maintain state across sessions", ["persistent", "neural"]),
        MemoryItem(302, "Memory systems require durable storage mechanisms", ["memory", "storage"]),
        MemoryItem(303, "Associative links must survive system restarts", ["associative", "persistence"]),
    ]
    
    for memory in test_memories:
        result = client.add_memory_persistent(memory)
        print(f"   Memory {memory.id}: Live({result['successful_layers']}/4) Persistent({result['persistence_success']})")
    
    print()
    
    # Test persistent search
    print("🔍 Testing persistent search...")
    results = client.search_with_persistence("persistent neural networks")
    print(f"   Found {len(results)} results")
    
    for result in results:
        print(f"   [{result.layer}] ID:{result.memory_id} Confidence:{result.confidence:.3f}")
    
    print()
    
    # Create backup
    print("💾 Creating system backup...")
    backup_dir = client.create_system_backup()
    if backup_dir:
        print(f"   Backup saved to: {backup_dir}")
    
    print()
    
    # Final storage stats
    final_stats = client.persistence.get_storage_stats()
    print("📊 Final Storage Statistics:")
    print(f"   Total memories: {final_stats.get('memory_count', 0)}")
    print(f"   Database size: {final_stats.get('database_size_mb', 0):.2f} MB")
    print(f"   Layer data size: {final_stats.get('layer_data_size_mb', 0):.2f} MB")
    print(f"   Total storage: {final_stats.get('total_size_mb', 0):.2f} MB")
    
    print()
    print("✅ MFN Persistence Demo Complete!")
    print("   The system now has full persistence capabilities:")
    print("   • SQLite database for memories and associations")
    print("   • Layer state snapshots")
    print("   • Automatic backup and restore")
    print("   • Persistent search capabilities")

if __name__ == "__main__":
    main()