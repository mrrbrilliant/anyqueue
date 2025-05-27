use anyqueue::{AnyQueue, GenericJobProcessor, Job, QueueStats, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::{signal, time};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct MonitoringJob {
    id: String,
    work_duration_ms: u64,
    should_fail: bool,
}

#[async_trait::async_trait]
impl Job for MonitoringJob {
    async fn process(&self) -> Result<()> {
        println!(
            "🔄 Processing job: {} (duration: {}ms)",
            self.id, self.work_duration_ms
        );

        // Simulate work
        time::sleep(Duration::from_millis(self.work_duration_ms)).await;

        if self.should_fail {
            println!("❌ Job {} failed intentionally", self.id);
            Err(anyqueue::Error::JobProcessing(
                "Intentional failure for monitoring demo".to_string(),
            ))
        } else {
            println!("✅ Job {} completed successfully", self.id);
            Ok(())
        }
    }
}

async fn monitor_queue_health(mut queue: AnyQueue) {
    loop {
        let health = queue.health_check().await;
        let stats = queue.get_stats().await.unwrap_or_else(|_| QueueStats {
            retry_queue_size: 0,
            dead_letter_queue_size: 0,
            estimated_jobs_due_now: 0,
            total_jobs_processed: None,
        });

        println!("\n📊 Queue Health Report:");
        println!(
            "   Redis Connected: {}",
            if health.redis_connected { "✅" } else { "❌" }
        );
        println!("   Retry Queue Size: {}", stats.retry_queue_size);
        println!(
            "   Dead Letter Queue Size: {}",
            stats.dead_letter_queue_size
        );
        println!("   Jobs Due Now: {}", stats.estimated_jobs_due_now);
        println!(
            "   Last Check: {}",
            chrono::DateTime::from_timestamp_millis(health.last_check_timestamp)
                .unwrap_or_default()
        );

        // Alert on high DLQ size
        if stats.dead_letter_queue_size > 5 {
            println!("⚠️  WARNING: High number of failed jobs in dead letter queue!");
        }

        if stats.retry_queue_size > 10 {
            println!("⚠️  WARNING: High number of pending jobs in retry queue!");
        }

        time::sleep(Duration::from_secs(5)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("🏥 AnyQueue Monitoring Demo");
    println!("===========================");

    // Create queue with configuration from environment or defaults
    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(2) // Lower for demo
        .base_delay(Duration::from_millis(500))
        .processor(GenericJobProcessor::<MonitoringJob>::new())
        .build()
        .await?;

    // Clear existing jobs for clean demo
    queue.clear_all_queues().await?;
    println!("🧹 Cleared existing queues");

    // Add test jobs with different characteristics
    let test_jobs = vec![
        MonitoringJob {
            id: "quick_success".to_string(),
            work_duration_ms: 100,
            should_fail: false,
        },
        MonitoringJob {
            id: "slow_success".to_string(),
            work_duration_ms: 2000,
            should_fail: false,
        },
        MonitoringJob {
            id: "failing_job_1".to_string(),
            work_duration_ms: 200,
            should_fail: true,
        },
        MonitoringJob {
            id: "failing_job_2".to_string(),
            work_duration_ms: 300,
            should_fail: true,
        },
        MonitoringJob {
            id: "another_success".to_string(),
            work_duration_ms: 500,
            should_fail: false,
        },
    ];

    // Enqueue jobs
    for job in test_jobs {
        let job_id = queue.enqueue(job).await?;
        println!("📝 Enqueued job: {}", job_id);
    }

    println!("\n🔄 Starting worker and monitoring...");
    println!("Press Ctrl+C to stop gracefully\n");

    // Start monitoring in background
    let monitoring_queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .processor(GenericJobProcessor::<MonitoringJob>::new())
        .build()
        .await?;

    let monitor_handle = tokio::spawn(monitor_queue_health(monitoring_queue));

    // Start worker
    let worker_handle = tokio::spawn(async move { queue.start_worker().await });

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\n🛑 Received Ctrl+C signal. Shutting down gracefully...");
        }
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
        }
    }

    // Graceful shutdown
    println!("🔄 Stopping worker...");
    worker_handle.abort();

    println!("🔄 Stopping monitoring...");
    monitor_handle.abort();

    println!("✅ Shutdown complete.");
    Ok(())
}
