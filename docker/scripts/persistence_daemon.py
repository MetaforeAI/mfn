#!/usr/bin/env python3
"""
MFN Persistence Daemon
Handles automatic persistence, backups, and recovery
"""

import os
import sys
import time
import json
import sqlite3
import threading
import logging
from datetime import datetime, timedelta
from typing import Dict, Any, Optional

# Add lib to path
sys.path.insert(0, '/app/lib')

from add_persistence import MFNPersistenceManager, PersistentMemory
from unified_socket_client import UnifiedMFNClient

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger('PersistenceDaemon')

class PersistenceDaemon:
    """Automatic persistence management daemon"""

    def __init__(self):
        self.data_dir = os.environ.get('MFN_DATA_DIR', '/app/data')
        self.backup_dir = os.environ.get('MFN_BACKUP_DIR', '/app/backups')

        # Initialize managers
        self.persistence_manager = MFNPersistenceManager(self.data_dir)
        self.mfn_client = None

        # Configuration
        self.auto_backup_enabled = True
        self.backup_interval_hours = 6
        self.checkpoint_interval_minutes = 5
        self.retention_days = 7

        # State tracking
        self.last_backup_time = None
        self.last_checkpoint_time = None
        self.running = True
        self.stats = {
            'checkpoints_created': 0,
            'backups_created': 0,
            'memories_persisted': 0,
            'errors': 0,
            'start_time': datetime.now()
        }

        # Start background threads
        self._start_background_tasks()

    def _start_background_tasks(self):
        """Start background maintenance tasks"""
        # Checkpoint thread
        checkpoint_thread = threading.Thread(
            target=self._checkpoint_loop,
            daemon=True
        )
        checkpoint_thread.start()

        # Backup thread
        backup_thread = threading.Thread(
            target=self._backup_loop,
            daemon=True
        )
        backup_thread.start()

        # Cleanup thread
        cleanup_thread = threading.Thread(
            target=self._cleanup_loop,
            daemon=True
        )
        cleanup_thread.start()

        logger.info("Background persistence tasks started")

    def _checkpoint_loop(self):
        """Periodic checkpoint creation"""
        while self.running:
            try:
                time.sleep(self.checkpoint_interval_minutes * 60)

                if self._create_checkpoint():
                    self.stats['checkpoints_created'] += 1
                    logger.info(f"Checkpoint created (total: {self.stats['checkpoints_created']})")

            except Exception as e:
                logger.error(f"Checkpoint failed: {e}")
                self.stats['errors'] += 1

    def _backup_loop(self):
        """Periodic backup creation"""
        while self.running:
            try:
                time.sleep(self.backup_interval_hours * 3600)

                if self.auto_backup_enabled and self._create_backup():
                    self.stats['backups_created'] += 1
                    logger.info(f"Backup created (total: {self.stats['backups_created']})")

            except Exception as e:
                logger.error(f"Backup failed: {e}")
                self.stats['errors'] += 1

    def _cleanup_loop(self):
        """Cleanup old backups and logs"""
        while self.running:
            try:
                time.sleep(86400)  # Daily cleanup

                self._cleanup_old_backups()
                self._cleanup_old_logs()
                self._optimize_database()

                logger.info("Cleanup tasks completed")

            except Exception as e:
                logger.error(f"Cleanup failed: {e}")
                self.stats['errors'] += 1

    def _create_checkpoint(self) -> bool:
        """Create incremental checkpoint"""
        try:
            checkpoint_file = os.path.join(
                self.data_dir,
                'checkpoints',
                f'checkpoint_{int(time.time())}.json'
            )

            os.makedirs(os.path.dirname(checkpoint_file), exist_ok=True)

            # Get current system state
            if self.mfn_client:
                try:
                    stats = self.mfn_client.get_system_stats()
                except:
                    stats = {}
            else:
                stats = {}

            # Create checkpoint
            checkpoint = {
                'timestamp': datetime.now().isoformat(),
                'system_stats': stats,
                'persistence_stats': self.persistence_manager.get_storage_stats(),
                'daemon_stats': self.stats
            }

            with open(checkpoint_file, 'w') as f:
                json.dump(checkpoint, f, indent=2)

            self.last_checkpoint_time = datetime.now()
            return True

        except Exception as e:
            logger.error(f"Checkpoint creation failed: {e}")
            return False

    def _create_backup(self) -> bool:
        """Create full backup"""
        try:
            backup_name = f"auto_backup_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
            backup_path = self.persistence_manager.create_backup(backup_name)

            if backup_path:
                self.last_backup_time = datetime.now()

                # Create backup metadata
                metadata_file = os.path.join(backup_path, 'backup_metadata_extended.json')
                metadata = {
                    'backup_name': backup_name,
                    'created_at': self.last_backup_time.isoformat(),
                    'auto_backup': True,
                    'daemon_stats': self.stats,
                    'storage_stats': self.persistence_manager.get_storage_stats()
                }

                with open(metadata_file, 'w') as f:
                    json.dump(metadata, f, indent=2)

                return True

            return False

        except Exception as e:
            logger.error(f"Backup creation failed: {e}")
            return False

    def _cleanup_old_backups(self):
        """Remove backups older than retention period"""
        try:
            backups_dir = os.path.join(self.data_dir, 'backups')
            if not os.path.exists(backups_dir):
                return

            retention_date = datetime.now() - timedelta(days=self.retention_days)

            for backup_dir in os.listdir(backups_dir):
                backup_path = os.path.join(backups_dir, backup_dir)

                if os.path.isdir(backup_path):
                    # Check backup age
                    metadata_file = os.path.join(backup_path, 'backup_metadata.json')

                    if os.path.exists(metadata_file):
                        with open(metadata_file, 'r') as f:
                            metadata = json.load(f)

                        backup_date = datetime.fromtimestamp(metadata.get('created_at', 0))

                        if backup_date < retention_date:
                            import shutil
                            shutil.rmtree(backup_path)
                            logger.info(f"Removed old backup: {backup_dir}")

        except Exception as e:
            logger.error(f"Backup cleanup failed: {e}")

    def _cleanup_old_logs(self):
        """Rotate and compress old logs"""
        try:
            log_dir = os.environ.get('MFN_LOG_DIR', '/app/logs')
            if not os.path.exists(log_dir):
                return

            retention_date = datetime.now() - timedelta(days=7)

            for log_file in os.listdir(log_dir):
                if log_file.endswith('.log'):
                    log_path = os.path.join(log_dir, log_file)
                    file_stat = os.stat(log_path)
                    file_date = datetime.fromtimestamp(file_stat.st_mtime)

                    if file_date < retention_date:
                        # Compress old log
                        import gzip
                        import shutil

                        with open(log_path, 'rb') as f_in:
                            with gzip.open(f'{log_path}.gz', 'wb') as f_out:
                                shutil.copyfileobj(f_in, f_out)

                        os.remove(log_path)
                        logger.info(f"Compressed old log: {log_file}")

        except Exception as e:
            logger.error(f"Log cleanup failed: {e}")

    def _optimize_database(self):
        """Optimize SQLite database"""
        try:
            db_path = os.path.join(self.data_dir, 'mfn_memories.db')
            if os.path.exists(db_path):
                conn = sqlite3.connect(db_path)
                cursor = conn.cursor()

                # Run optimization
                cursor.execute("VACUUM")
                cursor.execute("ANALYZE")

                # Update statistics
                cursor.execute("SELECT COUNT(*) FROM memories")
                memory_count = cursor.fetchone()[0]

                conn.close()

                logger.info(f"Database optimized (memories: {memory_count})")

        except Exception as e:
            logger.error(f"Database optimization failed: {e}")

    def handle_persistence_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Handle persistence API requests"""
        try:
            action = request.get('action')

            if action == 'backup':
                success = self._create_backup()
                return {
                    'success': success,
                    'message': 'Backup created' if success else 'Backup failed'
                }

            elif action == 'restore':
                backup_name = request.get('backup_name')
                backup_path = os.path.join(self.data_dir, 'backups', backup_name)
                success = self.persistence_manager.restore_from_backup(backup_path)
                return {
                    'success': success,
                    'message': 'Restore completed' if success else 'Restore failed'
                }

            elif action == 'stats':
                return {
                    'success': True,
                    'stats': {
                        'daemon': self.stats,
                        'storage': self.persistence_manager.get_storage_stats()
                    }
                }

            elif action == 'checkpoint':
                success = self._create_checkpoint()
                return {
                    'success': success,
                    'message': 'Checkpoint created' if success else 'Checkpoint failed'
                }

            else:
                return {
                    'success': False,
                    'message': f'Unknown action: {action}'
                }

        except Exception as e:
            logger.error(f"Request handling failed: {e}")
            return {
                'success': False,
                'message': str(e)
            }

    def monitor_memory_changes(self):
        """Monitor and persist memory changes"""
        try:
            # Initialize MFN client if needed
            if not self.mfn_client:
                try:
                    self.mfn_client = UnifiedMFNClient()
                except:
                    logger.warning("MFN client initialization failed, retrying later")
                    return

            # Get current memories from system
            current_memories = set()
            stats = self.mfn_client.get_system_stats()

            for layer_name, layer_stats in stats.items():
                if 'memory_count' in layer_stats:
                    # Track memory count changes
                    current_memories.add(layer_stats['memory_count'])

            # Persist any new memories
            # (This would need more sophisticated tracking in production)

        except Exception as e:
            logger.error(f"Memory monitoring failed: {e}")

    def run(self):
        """Main daemon loop"""
        logger.info("MFN Persistence Daemon started")
        logger.info(f"Data directory: {self.data_dir}")
        logger.info(f"Backup directory: {self.backup_dir}")
        logger.info(f"Auto-backup: {self.auto_backup_enabled}")
        logger.info(f"Backup interval: {self.backup_interval_hours} hours")
        logger.info(f"Checkpoint interval: {self.checkpoint_interval_minutes} minutes")
        logger.info(f"Retention: {self.retention_days} days")

        # Initial checkpoint
        self._create_checkpoint()

        # Main loop
        try:
            while self.running:
                # Monitor memory changes
                self.monitor_memory_changes()

                # Sleep
                time.sleep(10)

                # Log status periodically
                if int(time.time()) % 300 == 0:  # Every 5 minutes
                    uptime = (datetime.now() - self.stats['start_time']).total_seconds()
                    logger.info(
                        f"Status - Uptime: {uptime:.0f}s, "
                        f"Checkpoints: {self.stats['checkpoints_created']}, "
                        f"Backups: {self.stats['backups_created']}, "
                        f"Errors: {self.stats['errors']}"
                    )

        except KeyboardInterrupt:
            logger.info("Shutdown signal received")
            self.shutdown()

    def shutdown(self):
        """Graceful shutdown"""
        logger.info("Shutting down persistence daemon")
        self.running = False

        # Final checkpoint
        self._create_checkpoint()

        # Save final stats
        stats_file = os.path.join(self.data_dir, 'daemon_stats.json')
        with open(stats_file, 'w') as f:
            json.dump(self.stats, f, indent=2, default=str)

        logger.info("Persistence daemon shutdown complete")

if __name__ == "__main__":
    daemon = PersistenceDaemon()
    daemon.run()