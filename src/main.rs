use anyqueue::{AnyQueue, Job, Result, GenericJobProcessor};
use serde::{Deserialize, Serialize};
use tokio::signal;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DemoJob {
    id: String,
    message: String,
    should_fail: bool,
}

#[async_trait::async_trait]
impl Job for DemoJob {
    async fn process(&self) -> Result<()> {
        println!("🔄 Processing demo job: {} - {}", self.id, self.message);
        
        // Simulate some work
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        
        if self.should_fail {
            println!("❌ Job {} intentionally failed", self.id);
            Err(anyqueue::Error::JobProcessing("Intentional failure for demo".to_string()))
        } else {
            println!("✅ Job {} completed successfully", self.id);
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("🚀 AnyQueue Demo");
    println!("================");

    // Create queue with configuration and processor
    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(3)
        .base_delay(std::time::Duration::from_secs(2))
        .processor(GenericJobProcessor::<DemoJob>::new())
        .build()
        .await?;

    // Clear existing jobs for clean demo
    queue.clear_all_queues().await?;
    println!("🧹 Cleared existing queues");

    // Add demo jobs
    let demo_jobs = vec![
        DemoJob {
            id: "job_1".to_string(),
            message: "This job will succeed".to_string(),
            should_fail: false,
        },
        DemoJob {
            id: "job_2".to_string(),
            message: "This job will fail and retry".to_string(),
            should_fail: true,
        },
        DemoJob {
            id: "job_3".to_string(),
            message: "Another successful job".to_string(),
            should_fail: false,
        },
        DemoJob {
            id: "job_4".to_string(),
            message: "Another job that fails".to_string(),
            should_fail: true,
        },
    ];

    // Enqueue jobs
    for job in demo_jobs {
        let job_id = queue.enqueue(job).await?;
        println!("📝 Enqueued job: {}", job_id);
    }

    println!("📊 Jobs in retry queue: {}", queue.retry_queue_size().await?);
    println!("📊 Jobs in dead letter queue: {}", queue.dead_letter_queue_size().await?);
    
    // Start worker
    println!("\n🔄 Starting worker...");
    println!("Note: Failed jobs will be retried with exponential backoff");
    println!("Press Ctrl+C to stop\n");
    
    // Start worker with graceful shutdown handling
    let worker_handle = tokio::spawn(async move {
        queue.start_worker().await
    });

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\n🛑 Received Ctrl+C signal. Shutting down gracefully...");
        }
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
        }
    }

    // Cancel the worker task
    worker_handle.abort();
    
    println!("✅ Shutdown complete.");
    Ok(())
}
