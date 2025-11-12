package alm

import (
	"context"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"time"
)

// GoroutinePool manages a limited pool of goroutines to prevent exhaustion
type GoroutinePool struct {
	maxWorkers    int
	taskQueue     chan func()
	workerCount   int32
	activeCount   int32
	totalExecuted int64
	ctx           context.Context
	cancel        context.CancelFunc
	wg            sync.WaitGroup
	mu            sync.RWMutex
}

// NewGoroutinePool creates a new goroutine pool with a maximum worker limit
func NewGoroutinePool(maxWorkers int) *GoroutinePool {
	if maxWorkers <= 0 {
		maxWorkers = runtime.NumCPU() * 2
	}

	ctx, cancel := context.WithCancel(context.Background())

	pool := &GoroutinePool{
		maxWorkers: maxWorkers,
		taskQueue:  make(chan func(), maxWorkers*10), // Buffer for tasks
		ctx:        ctx,
		cancel:     cancel,
	}

	// Start initial workers (half of max to start)
	initialWorkers := maxWorkers / 2
	if initialWorkers < 1 {
		initialWorkers = 1
	}

	for i := 0; i < initialWorkers; i++ {
		pool.startWorker()
	}

	// Monitor and scale workers
	go pool.monitor()

	return pool
}

// Submit adds a task to the pool
func (p *GoroutinePool) Submit(task func()) error {
	select {
	case p.taskQueue <- task:
		// Check if we need more workers
		if len(p.taskQueue) > int(atomic.LoadInt32(&p.workerCount)) &&
		   int(atomic.LoadInt32(&p.workerCount)) < p.maxWorkers {
			p.startWorker()
		}
		return nil
	case <-p.ctx.Done():
		return context.Canceled
	default:
		// Queue is full, try to spawn emergency worker if under limit
		if int(atomic.LoadInt32(&p.workerCount)) < p.maxWorkers {
			p.startWorker()
			// Retry submission
			select {
			case p.taskQueue <- task:
				return nil
			case <-time.After(100 * time.Millisecond):
				return ErrPoolOverloaded
			}
		}
		return ErrPoolOverloaded
	}
}

// SubmitWithTimeout submits a task with a timeout
func (p *GoroutinePool) SubmitWithTimeout(task func(), timeout time.Duration) error {
	timer := time.NewTimer(timeout)
	defer timer.Stop()

	select {
	case p.taskQueue <- task:
		return nil
	case <-timer.C:
		return ErrSubmitTimeout
	case <-p.ctx.Done():
		return context.Canceled
	}
}

// startWorker starts a new worker goroutine
func (p *GoroutinePool) startWorker() {
	atomic.AddInt32(&p.workerCount, 1)
	p.wg.Add(1)

	go func() {
		defer func() {
			atomic.AddInt32(&p.workerCount, -1)
			p.wg.Done()
		}()

		idleTimer := time.NewTimer(30 * time.Second)
		defer idleTimer.Stop()

		for {
			idleTimer.Reset(30 * time.Second)

			select {
			case task := <-p.taskQueue:
				if task != nil {
					atomic.AddInt32(&p.activeCount, 1)
					func() {
						defer func() {
							if r := recover(); r != nil {
								// Log panic but don't crash the worker
								// fmt.Printf("Worker panic recovered: %v\n", r)
							}
							atomic.AddInt32(&p.activeCount, -1)
							atomic.AddInt64(&p.totalExecuted, 1)
						}()
						task()
					}()
				}

			case <-idleTimer.C:
				// Worker idle for too long, check if we should shut down
				if atomic.LoadInt32(&p.workerCount) > 1 && len(p.taskQueue) == 0 {
					return // Shut down this worker
				}

			case <-p.ctx.Done():
				return
			}
		}
	}()
}

// monitor periodically checks pool health and adjusts workers
func (p *GoroutinePool) monitor() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			queueLen := len(p.taskQueue)
			workerCount := atomic.LoadInt32(&p.workerCount)
			activeCount := atomic.LoadInt32(&p.activeCount)

			// Scale up if queue is backing up
			if queueLen > int(workerCount)*2 && int(workerCount) < p.maxWorkers {
				workersToAdd := (queueLen / int(workerCount)) - 1
				if workersToAdd > 5 {
					workersToAdd = 5 // Limit scaling rate
				}

				for i := 0; i < workersToAdd && int(atomic.LoadInt32(&p.workerCount)) < p.maxWorkers; i++ {
					p.startWorker()
				}
			}

			// Scale down if many workers are idle
			targetWorkers := int(activeCount) + 2 // Keep a small buffer
			if targetWorkers < p.maxWorkers/4 {
				targetWorkers = p.maxWorkers / 4 // Keep minimum workers
			}

			// Scaling down happens naturally via idle timeout in workers

		case <-p.ctx.Done():
			return
		}
	}
}

// GetStats returns pool statistics
func (p *GoroutinePool) GetStats() PoolStats {
	return PoolStats{
		WorkerCount:    atomic.LoadInt32(&p.workerCount),
		ActiveCount:    atomic.LoadInt32(&p.activeCount),
		QueueLength:    len(p.taskQueue),
		TotalExecuted:  atomic.LoadInt64(&p.totalExecuted),
		MaxWorkers:     p.maxWorkers,
	}
}

// Close shuts down the pool gracefully
func (p *GoroutinePool) Close() error {
	p.cancel()

	// Wait for tasks to complete with timeout
	done := make(chan struct{})
	go func() {
		p.wg.Wait()
		close(done)
	}()

	select {
	case <-done:
		return nil
	case <-time.After(30 * time.Second):
		return ErrShutdownTimeout
	}
}

// PoolStats contains statistics about the goroutine pool
type PoolStats struct {
	WorkerCount   int32 `json:"worker_count"`
	ActiveCount   int32 `json:"active_count"`
	QueueLength   int   `json:"queue_length"`
	TotalExecuted int64 `json:"total_executed"`
	MaxWorkers    int   `json:"max_workers"`
}

// Pool errors
var (
	ErrPoolOverloaded  = fmt.Errorf("goroutine pool overloaded")
	ErrSubmitTimeout   = fmt.Errorf("task submission timed out")
	ErrShutdownTimeout = fmt.Errorf("pool shutdown timed out")
)