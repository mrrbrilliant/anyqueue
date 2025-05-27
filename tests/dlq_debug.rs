use anyqueue::{AnyQueue, GenericJobProcessor, Job, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DebugJob {
    id: String,
    should_succeed: bool,
}

impl DebugJob {
    fn new(id: &str, should_succeed: bool) -> Self {
        Self {
            id: id.to_string(),
            should_succeed,
        }
    }
}

#[async_trait::async_trait]
impl Job for DebugJob {
    async fn process(&self) -> Result<()> {
        println!("🔧 Processing DebugJob {}", self.id);
        if self.should_succeed {
            println!("✅ DebugJob {} succeeded", self.id);
            Ok(())
        } else {
            println!("❌ DebugJob {} failed", self.id);
            Err(anyqueue::Error::JobProcessing("Test job failure".into()))
        }
    }
}

#[tokio::test]
async fn debug_dlq_detailed() -> Result<()> {
    // Setup queue with detailed logging
    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(1) // Allow only 1 retry
        .base_delay(Duration::from_millis(10)) // Very short delay
        .processor(GenericJobProcessor::<DebugJob>::new())
        .build()
        .await?;

    queue.clear_all_queues().await?;
    println!("🧹 Cleared all queues");

    // Enqueue failing job
    let job = DebugJob::new("debug_dlq_job", false);
    queue.enqueue(job).await?;
    println!(
        "📨 Enqueued job, retry queue size: {}",
        queue.retry_queue_size().await?
    );

    // Start worker in a controlled way
    let worker_handle = tokio::spawn(async move {
        println!("🏃 Starting worker...");
        if let Err(e) = queue.start_worker().await {
            println!("⚠️ Worker error: {}", e);
        }
    });

    // Give time for processing cycles
    for i in 1..=5 {
        time::sleep(Duration::from_millis(500)).await;

        let mut check_queue = AnyQueue::builder()
            .redis_url("redis://127.0.0.1:6379")
            .max_retries(1)
            .base_delay(Duration::from_millis(10))
            .processor(GenericJobProcessor::<DebugJob>::new())
            .build()
            .await?;

        let retry_size = check_queue.retry_queue_size().await?;
        let dlq_size = check_queue.dead_letter_queue_size().await?;

        println!(
            "🔍 Cycle {}: Retry queue: {}, DLQ: {}",
            i, retry_size, dlq_size
        );

        if dlq_size > 0 {
            println!("🎯 Found job in DLQ!");
            break;
        }

        if retry_size == 0 && dlq_size == 0 {
            println!("⚠️ Job disappeared from both queues!");
            break;
        }
    }

    worker_handle.abort();

    // Final check
    let mut final_queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(1)
        .base_delay(Duration::from_millis(10))
        .processor(GenericJobProcessor::<DebugJob>::new())
        .build()
        .await?;

    let final_retry_size = final_queue.retry_queue_size().await?;
    let final_dlq_size = final_queue.dead_letter_queue_size().await?;

    println!(
        "🏁 Final state - Retry queue: {}, DLQ: {}",
        final_retry_size, final_dlq_size
    );

    // The job should be in DLQ after max_retries
    assert!(
        final_dlq_size > 0,
        "Job should be in DLQ after exceeding max retries"
    );

    Ok(())
}
