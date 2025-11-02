#!/bin/bash

echo "🛑 Stopping all MFN layer socket servers..."

# Kill processes
for i in 1 2 3 4; do
    if [ -f "/tmp/mfn_layer${i}.pid" ]; then
        PID=$(cat /tmp/mfn_layer${i}.pid)
        if kill -0 $PID 2>/dev/null; then
            echo "Stopping Layer ${i} (PID: $PID)..."
            kill $PID
            rm /tmp/mfn_layer${i}.pid
        else
            echo "Layer ${i} already stopped"
            rm /tmp/mfn_layer${i}.pid
        fi
    fi
done

# Clean up sockets
rm -f /tmp/mfn_layer*.sock

echo "✅ All layers stopped"
