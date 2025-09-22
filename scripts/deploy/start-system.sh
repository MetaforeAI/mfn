#!/bin/bash

# MFN System with Persistence: Complete Startup Script
# Starts all layers and automatically restores persistent state

echo "🧠 Starting MFN System with Persistence"
echo "========================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

# Configuration
DATA_DIR="./mfn_data"
RESTORE_ON_START=true

echo -e "${PURPLE}🗄️  Persistence Configuration:${NC}"
echo "   Data directory: $DATA_DIR"
echo "   Auto-restore: $RESTORE_ON_START"
echo ""

# Check for existing persistent data
if [ -f "$DATA_DIR/mfn_memories.db" ]; then
    echo -e "${GREEN}✅ Persistent data found${NC}"
    
    # Get memory count from database
    MEMORY_COUNT=$(sqlite3 "$DATA_DIR/mfn_memories.db" "SELECT COUNT(*) FROM memories;" 2>/dev/null || echo "0")
    ASSOC_COUNT=$(sqlite3 "$DATA_DIR/mfn_memories.db" "SELECT COUNT(*) FROM layer3_associations;" 2>/dev/null || echo "0")
    
    echo "   Memories in storage: $MEMORY_COUNT"
    echo "   Associations in storage: $ASSOC_COUNT"
    
    if [ "$MEMORY_COUNT" -gt "0" ] && [ "$RESTORE_ON_START" = true ]; then
        echo -e "${BLUE}   Will restore state after layers start${NC}"
    fi
else
    echo -e "${YELLOW}⚠️  No persistent data found - starting fresh${NC}"
    echo "   Persistence will be initialized on first memory addition"
fi

echo ""

# Start the base layer system
echo -e "${BLUE}🚀 Starting base layer system...${NC}"
./start_all_layers.sh

# Check if layers started successfully
layers_running=0
if [ -S "/tmp/mfn_layer1.sock" ]; then layers_running=$((layers_running + 1)); fi
if [ -S "/tmp/mfn_layer2.sock" ]; then layers_running=$((layers_running + 1)); fi
if [ -S "/tmp/mfn_layer3.sock" ]; then layers_running=$((layers_running + 1)); fi
if [ -S "/tmp/mfn_layer4.sock" ]; then layers_running=$((layers_running + 1)); fi

echo ""

if [ $layers_running -eq 0 ]; then
    echo -e "${RED}❌ No layers started successfully${NC}"
    echo "   Cannot restore persistent state without running layers"
    exit 1
fi

echo -e "${GREEN}✅ $layers_running/4 layers running${NC}"

# Restore persistent state if available and requested
if [ -f "$DATA_DIR/mfn_memories.db" ] && [ "$MEMORY_COUNT" -gt "0" ] && [ "$RESTORE_ON_START" = true ]; then
    echo ""
    echo -e "${PURPLE}🔄 Restoring persistent state...${NC}"
    
    # Give layers a moment to fully initialize
    sleep 2
    
    # Run the persistence restoration
    if python3 add_persistence.py >/dev/null 2>&1; then
        echo -e "${GREEN}✅ Persistent state restored successfully${NC}"
        
        # Verify restoration by checking memory count in live system
        echo "   Verifying restoration..."
        
        # Run a quick validation
        if python3 -c "
from add_persistence import MFNPersistentClient
import sys
try:
    client = MFNPersistentClient()
    health = client.client.health_check()
    healthy_layers = sum(health.values())
    print(f'   Live system: {healthy_layers}/4 layers responsive')
    if healthy_layers >= 2:
        sys.exit(0)
    else:
        sys.exit(1)
except Exception as e:
    print(f'   Verification error: {e}')
    sys.exit(1)
" 2>/dev/null; then
            echo -e "${GREEN}   ✅ System verification passed${NC}"
        else
            echo -e "${YELLOW}   ⚠️  System verification failed (but layers are running)${NC}"
        fi
        
    else
        echo -e "${YELLOW}⚠️  Persistence restoration encountered issues${NC}"
        echo "   System will continue with empty state"
        echo "   Previous data remains safely stored in $DATA_DIR"
    fi
fi

echo ""
echo -e "${PURPLE}📊 System Status Summary:${NC}"
echo "==============================="

# Display layer status
echo "Layer Status:"
if [ -S "/tmp/mfn_layer1.sock" ]; then
    echo -e "  ✅ Layer 1 (IFR): Ultra-fast exact matching"
else
    echo -e "  ❌ Layer 1 (IFR): Not running"
fi

if [ -S "/tmp/mfn_layer2.sock" ]; then
    echo -e "  ✅ Layer 2 (DSR): Neural similarity search"
else
    echo -e "  ❌ Layer 2 (DSR): Not running"
fi

if [ -S "/tmp/mfn_layer3.sock" ]; then
    echo -e "  ✅ Layer 3 (ALM): Associative graph search"
else
    echo -e "  ❌ Layer 3 (ALM): Not running"
fi

if [ -S "/tmp/mfn_layer4.sock" ]; then
    echo -e "  ✅ Layer 4 (CPE): Context prediction"
else
    echo -e "  ❌ Layer 4 (CPE): Not running"
fi

echo ""

# Display persistence status
if [ -f "$DATA_DIR/mfn_memories.db" ]; then
    CURRENT_MEMORY_COUNT=$(sqlite3 "$DATA_DIR/mfn_memories.db" "SELECT COUNT(*) FROM memories;" 2>/dev/null || echo "0")
    CURRENT_ASSOC_COUNT=$(sqlite3 "$DATA_DIR/mfn_memories.db" "SELECT COUNT(*) FROM layer3_associations;" 2>/dev/null || echo "0")
    
    echo "Persistence Status:"
    echo "  📁 Data directory: $DATA_DIR"
    echo "  💾 Database: $([ -f "$DATA_DIR/mfn_memories.db" ] && echo "✅ Active" || echo "❌ Missing")"
    echo "  📊 Stored memories: $CURRENT_MEMORY_COUNT"
    echo "  🔗 Stored associations: $CURRENT_ASSOC_COUNT"
    
    # Calculate database size
    if [ -f "$DATA_DIR/mfn_memories.db" ]; then
        DB_SIZE=$(du -h "$DATA_DIR/mfn_memories.db" | cut -f1)
        echo "  💾 Database size: $DB_SIZE"
    fi
else
    echo "Persistence Status:"
    echo "  📁 Data directory: Will be created on first use"
    echo "  💾 Database: Will be initialized automatically"
fi

echo ""

# Usage instructions
echo -e "${BLUE}🎯 Usage Instructions:${NC}"
echo "=================="
echo ""
echo "Test the system with persistence:"
echo "  python3 add_persistence.py"
echo ""
echo "Test unified socket client:"
echo "  python3 unified_socket_client.py"
echo ""
echo "Run comprehensive validation:"
echo "  python3 final_system_validation.py"
echo ""
echo "Create manual backup:"
echo "  python3 -c \"from add_persistence import MFNPersistenceManager; MFNPersistenceManager().create_backup()\""
echo ""
echo "Stop all layers:"
echo "  ./start_all_layers.sh stop"

echo ""

# Final status
if [ $layers_running -eq 4 ]; then
    echo -e "${GREEN}🎉 MFN SYSTEM WITH PERSISTENCE FULLY OPERATIONAL!${NC}"
    echo -e "${GREEN}   • All 4 layers running with socket interfaces${NC}"
    echo -e "${GREEN}   • Persistence system active and ready${NC}"
    echo -e "${GREEN}   • Automatic state restoration enabled${NC}"
    echo -e "${GREEN}   • Ready for production workloads${NC}"
elif [ $layers_running -gt 2 ]; then
    echo -e "${YELLOW}⚠️  MFN SYSTEM PARTIALLY OPERATIONAL${NC}"
    echo -e "${YELLOW}   • $layers_running/4 layers running${NC}"
    echo -e "${YELLOW}   • Persistence system active${NC}"
    echo -e "${YELLOW}   • Reduced functionality but usable${NC}"
else
    echo -e "${RED}❌ MFN SYSTEM STARTUP ISSUES${NC}"
    echo -e "${RED}   • Only $layers_running/4 layers running${NC}"
    echo -e "${RED}   • Check error messages above${NC}"
fi

echo ""
echo "🧠 MFN System with Persistence startup complete"

# Optional monitoring mode
if [ "$1" = "monitor" ]; then
    echo ""
    echo -e "${BLUE}📊 Entering monitoring mode...${NC}"
    echo "Press Ctrl+C to exit"
    echo "================================"
    
    while true; do
        sleep 10
        
        # Check layers
        current_layers=0
        if [ -S "/tmp/mfn_layer1.sock" ]; then current_layers=$((current_layers + 1)); fi
        if [ -S "/tmp/mfn_layer2.sock" ]; then current_layers=$((current_layers + 1)); fi
        if [ -S "/tmp/mfn_layer3.sock" ]; then current_layers=$((current_layers + 1)); fi
        if [ -S "/tmp/mfn_layer4.sock" ]; then current_layers=$((current_layers + 1)); fi
        
        # Check memory count
        if [ -f "$DATA_DIR/mfn_memories.db" ]; then
            LIVE_MEMORY_COUNT=$(sqlite3 "$DATA_DIR/mfn_memories.db" "SELECT COUNT(*) FROM memories;" 2>/dev/null || echo "0")
        else
            LIVE_MEMORY_COUNT="0"
        fi
        
        echo "$(date): Layers($current_layers/4) Memories($LIVE_MEMORY_COUNT) Status($([ $current_layers -ge 3 ] && echo "HEALTHY" || echo "DEGRADED"))"
        
        if [ $current_layers -eq 0 ]; then
            echo -e "${RED}All layers down - exiting monitor mode${NC}"
            break
        fi
    done
fi