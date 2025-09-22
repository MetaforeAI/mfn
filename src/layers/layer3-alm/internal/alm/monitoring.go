package alm

import (
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

// PerformanceMonitor provides comprehensive performance tracking
type PerformanceMonitor struct {
	// Search performance tracking
	searchLatency    *LatencyTracker
	memoryLatency    *LatencyTracker
	graphLatency     *LatencyTracker
	
	// Throughput counters
	searchThroughput    *ThroughputTracker
	memoryThroughput    *ThroughputTracker
	
	// Resource usage tracking
	memoryUsage      *ResourceTracker
	connectionUsage  *ResourceTracker
	goroutineUsage   *ResourceTracker
	
	// Cache performance
	cacheMetrics     *CacheMetrics
	
	// Error tracking
	errorTracker     *ErrorTracker
	
	// Background monitoring
	stopChan chan struct{}
	wg       sync.WaitGroup
}

// LatencyTracker tracks response time percentiles
type LatencyTracker struct {
	name      string
	histogram prometheus.Histogram
	
	// Internal tracking
	samples   []float64
	mu        sync.RWMutex
	totalTime int64
	count     int64
	
	// Real-time percentiles
	p50, p95, p99, p999 float64
}

// ThroughputTracker measures operations per second
type ThroughputTracker struct {
	name       string
	counter    prometheus.Counter
	gauge      prometheus.Gauge
	
	// Sliding window for real-time throughput
	window     []int64
	windowSize int
	windowIdx  int
	mu         sync.Mutex
	lastUpdate time.Time
}

// ResourceTracker monitors system resources
type ResourceTracker struct {
	name         string
	gauge        prometheus.Gauge
	currentValue int64
	peakValue    int64
	mu           sync.RWMutex
}

// CacheMetrics tracks cache performance
type CacheMetrics struct {
	hitRate      prometheus.Gauge
	missRate     prometheus.Gauge
	evictionRate prometheus.Counter
	
	hits         int64
	misses       int64
	evictions    int64
}

// ErrorTracker monitors error rates and types
type ErrorTracker struct {
	totalErrors   prometheus.Counter
	errorsByType  *prometheus.CounterVec
	errorRate     prometheus.Gauge
	
	recentErrors []error
	mu           sync.RWMutex
}

// NewPerformanceMonitor creates a comprehensive performance monitor
func NewPerformanceMonitor() *PerformanceMonitor {
	monitor := &PerformanceMonitor{
		searchLatency:    NewLatencyTracker("search", "Search operation latency"),
		memoryLatency:    NewLatencyTracker("memory", "Memory operation latency"),
		graphLatency:     NewLatencyTracker("graph", "Graph operation latency"),
		
		searchThroughput: NewThroughputTracker("search", "Search operations per second"),
		memoryThroughput: NewThroughputTracker("memory", "Memory operations per second"),
		
		memoryUsage:     NewResourceTracker("memory", "Memory usage in bytes"),
		connectionUsage: NewResourceTracker("connections", "Active connections"),
		goroutineUsage:  NewResourceTracker("goroutines", "Number of goroutines"),
		
		cacheMetrics: NewCacheMetrics(),
		errorTracker: NewErrorTracker(),
		
		stopChan: make(chan struct{}),
	}
	
	monitor.startBackgroundMonitoring()
	return monitor
}

// NewLatencyTracker creates a new latency tracker
func NewLatencyTracker(name, help string) *LatencyTracker {
	return &LatencyTracker{
		name: name,
		histogram: promauto.NewHistogram(prometheus.HistogramOpts{
			Name:    fmt.Sprintf("alm_optimized_%s_duration_seconds", name),
			Help:    help,
			Buckets: prometheus.ExponentialBuckets(0.0001, 2, 15), // 0.1ms to ~3.2s
		}),
		samples: make([]float64, 0, 10000),
	}
}

// NewThroughputTracker creates a new throughput tracker
func NewThroughputTracker(name, help string) *ThroughputTracker {
	return &ThroughputTracker{
		name: name,
		counter: promauto.NewCounter(prometheus.CounterOpts{
			Name: fmt.Sprintf("alm_optimized_%s_total", name),
			Help: fmt.Sprintf("Total %s", help),
		}),
		gauge: promauto.NewGauge(prometheus.GaugeOpts{
			Name: fmt.Sprintf("alm_optimized_%s_per_second", name),
			Help: help,
		}),
		window:     make([]int64, 60), // 60 second window
		windowSize: 60,
		lastUpdate: time.Now(),
	}
}

// NewResourceTracker creates a new resource tracker
func NewResourceTracker(name, help string) *ResourceTracker {
	return &ResourceTracker{
		name: name,
		gauge: promauto.NewGauge(prometheus.GaugeOpts{
			Name: fmt.Sprintf("alm_optimized_%s_current", name),
			Help: help,
		}),
	}
}

// NewCacheMetrics creates cache performance metrics
func NewCacheMetrics() *CacheMetrics {
	return &CacheMetrics{
		hitRate: promauto.NewGauge(prometheus.GaugeOpts{
			Name: "alm_optimized_cache_hit_rate",
			Help: "Cache hit rate percentage",
		}),
		missRate: promauto.NewGauge(prometheus.GaugeOpts{
			Name: "alm_optimized_cache_miss_rate", 
			Help: "Cache miss rate percentage",
		}),
		evictionRate: promauto.NewCounter(prometheus.CounterOpts{
			Name: "alm_optimized_cache_evictions_total",
			Help: "Total number of cache evictions",
		}),
	}
}

// NewErrorTracker creates an error tracker
func NewErrorTracker() *ErrorTracker {
	return &ErrorTracker{
		totalErrors: promauto.NewCounter(prometheus.CounterOpts{
			Name: "alm_optimized_errors_total",
			Help: "Total number of errors",
		}),
		errorsByType: promauto.NewCounterVec(prometheus.CounterOpts{
			Name: "alm_optimized_errors_by_type_total",
			Help: "Errors by type",
		}, []string{"type"}),
		errorRate: promauto.NewGauge(prometheus.GaugeOpts{
			Name: "alm_optimized_error_rate",
			Help: "Current error rate percentage",
		}),
		recentErrors: make([]error, 0, 100),
	}
}

// RecordLatency records a latency measurement
func (lt *LatencyTracker) RecordLatency(duration time.Duration) {
	seconds := duration.Seconds()
	lt.histogram.Observe(seconds)
	
	lt.mu.Lock()
	defer lt.mu.Unlock()
	
	atomic.AddInt64(&lt.totalTime, duration.Nanoseconds())
	atomic.AddInt64(&lt.count, 1)
	
	// Add to samples for percentile calculation
	lt.samples = append(lt.samples, seconds)
	
	// Limit sample size to prevent memory growth
	if len(lt.samples) > 10000 {
		// Keep recent 5000 samples
		copy(lt.samples, lt.samples[5000:])
		lt.samples = lt.samples[:5000]
	}
	
	// Update percentiles every 100 samples
	if len(lt.samples)%100 == 0 {
		lt.updatePercentiles()
	}
}

// updatePercentiles calculates current percentiles
func (lt *LatencyTracker) updatePercentiles() {
	if len(lt.samples) == 0 {
		return
	}
	
	// Simple percentile calculation
	sorted := make([]float64, len(lt.samples))
	copy(sorted, lt.samples)
	
	// Quick sort approximation for percentiles
	n := len(sorted)
	lt.p50 = sorted[n*50/100]
	lt.p95 = sorted[n*95/100]
	lt.p99 = sorted[n*99/100]
	lt.p999 = sorted[n*999/1000]
}

// GetStats returns current latency statistics
func (lt *LatencyTracker) GetStats() map[string]float64 {
	lt.mu.RLock()
	defer lt.mu.RUnlock()
	
	count := atomic.LoadInt64(&lt.count)
	if count == 0 {
		return map[string]float64{
			"count":    0,
			"average":  0,
			"p50":      0,
			"p95":      0,
			"p99":      0,
			"p999":     0,
		}
	}
	
	totalTime := atomic.LoadInt64(&lt.totalTime)
	avgMs := float64(totalTime) / float64(count) / 1e6 // Convert to milliseconds
	
	return map[string]float64{
		"count":    float64(count),
		"average":  avgMs,
		"p50":      lt.p50 * 1000, // Convert to milliseconds
		"p95":      lt.p95 * 1000,
		"p99":      lt.p99 * 1000,
		"p999":     lt.p999 * 1000,
	}
}

// RecordOperation records a throughput operation
func (tt *ThroughputTracker) RecordOperation() {
	tt.counter.Inc()
	
	tt.mu.Lock()
	defer tt.mu.Unlock()
	
	now := time.Now()
	second := now.Unix()
	
	// Update window
	idx := int(second % int64(tt.windowSize))
	if idx != tt.windowIdx {
		// Clear slots between last update and now
		for i := (tt.windowIdx + 1) % tt.windowSize; i != idx; i = (i + 1) % tt.windowSize {
			tt.window[i] = 0
		}
		tt.windowIdx = idx
		tt.window[idx] = 0
	}
	
	tt.window[idx]++
	tt.lastUpdate = now
	
	// Calculate and update current throughput
	total := int64(0)
	for _, count := range tt.window {
		total += count
	}
	
	tt.gauge.Set(float64(total) / float64(tt.windowSize))
}

// GetThroughput returns current throughput
func (tt *ThroughputTracker) GetThroughput() float64 {
	tt.mu.Lock()
	defer tt.mu.Unlock()
	
	total := int64(0)
	for _, count := range tt.window {
		total += count
	}
	
	return float64(total) / float64(tt.windowSize)
}

// UpdateValue updates a resource value
func (rt *ResourceTracker) UpdateValue(value int64) {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	
	rt.currentValue = value
	if value > rt.peakValue {
		rt.peakValue = value
	}
	
	rt.gauge.Set(float64(value))
}

// GetStats returns resource statistics
func (rt *ResourceTracker) GetStats() map[string]int64 {
	rt.mu.RLock()
	defer rt.mu.RUnlock()
	
	return map[string]int64{
		"current": rt.currentValue,
		"peak":    rt.peakValue,
	}
}

// RecordHit records a cache hit
func (cm *CacheMetrics) RecordHit() {
	atomic.AddInt64(&cm.hits, 1)
	cm.updateRates()
}

// RecordMiss records a cache miss
func (cm *CacheMetrics) RecordMiss() {
	atomic.AddInt64(&cm.misses, 1)
	cm.updateRates()
}

// RecordEviction records a cache eviction
func (cm *CacheMetrics) RecordEviction() {
	atomic.AddInt64(&cm.evictions, 1)
	cm.evictionRate.Inc()
}

// updateRates updates hit/miss rates
func (cm *CacheMetrics) updateRates() {
	hits := atomic.LoadInt64(&cm.hits)
	misses := atomic.LoadInt64(&cm.misses)
	total := hits + misses
	
	if total > 0 {
		hitRate := float64(hits) / float64(total) * 100
		missRate := float64(misses) / float64(total) * 100
		
		cm.hitRate.Set(hitRate)
		cm.missRate.Set(missRate)
	}
}

// GetStats returns cache statistics
func (cm *CacheMetrics) GetStats() map[string]interface{} {
	hits := atomic.LoadInt64(&cm.hits)
	misses := atomic.LoadInt64(&cm.misses)
	evictions := atomic.LoadInt64(&cm.evictions)
	total := hits + misses
	
	hitRate := float64(0)
	if total > 0 {
		hitRate = float64(hits) / float64(total) * 100
	}
	
	return map[string]interface{}{
		"hits":       hits,
		"misses":     misses,
		"evictions":  evictions,
		"hit_rate":   hitRate,
		"total":      total,
	}
}

// RecordError records an error
func (et *ErrorTracker) RecordError(err error, errorType string) {
	et.totalErrors.Inc()
	et.errorsByType.WithLabelValues(errorType).Inc()
	
	et.mu.Lock()
	defer et.mu.Unlock()
	
	et.recentErrors = append(et.recentErrors, err)
	if len(et.recentErrors) > 100 {
		et.recentErrors = et.recentErrors[1:]
	}
}

// GetRecentErrors returns recent errors
func (et *ErrorTracker) GetRecentErrors() []error {
	et.mu.RLock()
	defer et.mu.RUnlock()
	
	errors := make([]error, len(et.recentErrors))
	copy(errors, et.recentErrors)
	return errors
}

// startBackgroundMonitoring starts background monitoring tasks
func (pm *PerformanceMonitor) startBackgroundMonitoring() {
	pm.wg.Add(1)
	go func() {
		defer pm.wg.Done()
		
		ticker := time.NewTicker(1 * time.Second)
		defer ticker.Stop()
		
		for {
			select {
			case <-pm.stopChan:
				return
			case <-ticker.C:
				pm.updateSystemMetrics()
			}
		}
	}()
}

// updateSystemMetrics updates system-level metrics
func (pm *PerformanceMonitor) updateSystemMetrics() {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	
	// Update memory usage
	pm.memoryUsage.UpdateValue(int64(m.Alloc))
	
	// Update goroutine count
	pm.goroutineUsage.UpdateValue(int64(runtime.NumGoroutine()))
}

// GetComprehensiveStats returns all performance statistics
func (pm *PerformanceMonitor) GetComprehensiveStats() map[string]interface{} {
	return map[string]interface{}{
		"latency": map[string]interface{}{
			"search": pm.searchLatency.GetStats(),
			"memory": pm.memoryLatency.GetStats(),
			"graph":  pm.graphLatency.GetStats(),
		},
		"throughput": map[string]interface{}{
			"search": pm.searchThroughput.GetThroughput(),
			"memory": pm.memoryThroughput.GetThroughput(),
		},
		"resources": map[string]interface{}{
			"memory":      pm.memoryUsage.GetStats(),
			"connections": pm.connectionUsage.GetStats(),
			"goroutines":  pm.goroutineUsage.GetStats(),
		},
		"cache": pm.cacheMetrics.GetStats(),
		"errors": map[string]interface{}{
			"recent": len(pm.errorTracker.GetRecentErrors()),
		},
		"timestamp": time.Now(),
	}
}

// RecordSearchLatency records search operation latency
func (pm *PerformanceMonitor) RecordSearchLatency(duration time.Duration) {
	pm.searchLatency.RecordLatency(duration)
	pm.searchThroughput.RecordOperation()
}

// RecordMemoryLatency records memory operation latency
func (pm *PerformanceMonitor) RecordMemoryLatency(duration time.Duration) {
	pm.memoryLatency.RecordLatency(duration)
	pm.memoryThroughput.RecordOperation()
}

// RecordGraphLatency records graph operation latency
func (pm *PerformanceMonitor) RecordGraphLatency(duration time.Duration) {
	pm.graphLatency.RecordLatency(duration)
}

// RecordCacheHit records a cache hit
func (pm *PerformanceMonitor) RecordCacheHit() {
	pm.cacheMetrics.RecordHit()
}

// RecordCacheMiss records a cache miss
func (pm *PerformanceMonitor) RecordCacheMiss() {
	pm.cacheMetrics.RecordMiss()
}

// RecordError records an error with type
func (pm *PerformanceMonitor) RecordError(err error, errorType string) {
	pm.errorTracker.RecordError(err, errorType)
}

// UpdateConnections updates the active connection count
func (pm *PerformanceMonitor) UpdateConnections(count int64) {
	pm.connectionUsage.UpdateValue(count)
}

// Stop stops the performance monitor
func (pm *PerformanceMonitor) Stop() {
	close(pm.stopChan)
	pm.wg.Wait()
}