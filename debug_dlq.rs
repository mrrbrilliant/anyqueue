use anyqueue::{AnyQueue, Job, GenericJobProcessor, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SimpleTestJob {
    id: String,
}

#[async_trait::async_trait]
impl Job for SimpleTestJob {
    async fn process(&self) -> Result<()> {
        println!("Processing job: {}", self.id);
        // Always fail to test DLQ
        Err(anyqueue::Error::JobProcessing("Intentional failure".to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let processor = GenericJobProcessor::<SimpleTestJob>::new();
    println!("Processor job type: {}", processor.job_type());

    let job = SimpleTestJob { id: "test-1".to_string() };
    println!("Job type: {}", job.job_type());

    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(1)
        .base_delay(std::time::Duration::from_millis(0))
        .processor(processor)
        .build()
        .await?;

    queue.clear_all_queues().await?;

    let job_id = queue.enqueue(job).await?;
    println!("Enqueued job: {}", job_id);

    println!("Queue size: {}", queue.retry_queue_size().await?);

    // Start worker in background
    let worker_task = tokio::spawn(async move {
        queue.start_worker().await
    });

    // Give it time to process
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
    worker_task.abort();

    // Check results
    let mut check_queue = AnyQueue::new().await?;
    println!("Final retry queue size: {}", check_queue.retry_queue_size().await?);
    println!("Final DLQ size: {}", check_queue.dead_letter_queue_size().await?);

    Ok(())
}
