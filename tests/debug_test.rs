use anyqueue::{AnyQueue, Job, GenericJobProcessor, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
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
        println!("🔍 DEBUG: Processing job {}", self.id);
        
        if self.should_succeed {
            println!("✅ DEBUG: Job {} succeeded", self.id);
            Ok(())
        } else {
            println!("❌ DEBUG: Job {} failed", self.id);
            Err(anyqueue::Error::JobProcessing(format!(
                "Job {} intentionally failed",
                self.id
            )))
        }
    }
}

// Custom processor that tracks job executions
struct DebugJobProcessor {
    inner: GenericJobProcessor<DebugJob>,
    counter: Arc<Mutex<Vec<String>>>,
}

impl DebugJobProcessor {
    fn new(counter: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            inner: GenericJobProcessor::new(),
            counter,
        }
    }
}

#[async_trait::async_trait]
impl anyqueue::JobProcessor for DebugJobProcessor {
    fn job_type(&self) -> &'static str {
        self.inner.job_type()
    }

    async fn process(&self, job_data: &anyqueue::JobData) -> Result<()> {
        let job: DebugJob = job_data.deserialize_job()?;
        
        println!("🔍 DEBUG: Processor tracking job: {}", job.id);
        
        // Track execution
        {
            let mut counter = self.counter.lock().unwrap();
            counter.push(job.id.clone());
            println!("🔍 DEBUG: Current execution count: {}", counter.len());
        }

        self.inner.process(job_data).await
    }
}

#[tokio::test]
async fn debug_test() -> Result<()> {
    env_logger::init();
    
    println!("🔍 DEBUG: Starting debug test");
    
    let counter = Arc::new(Mutex::new(Vec::new()));
    let processor = DebugJobProcessor::new(counter.clone());

    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(3)
        .base_delay(Duration::from_millis(0)) // No delay for immediate processing
        .processor(processor)
        .build()
        .await?;

    queue.clear_all_queues().await?;
    println!("🔍 DEBUG: Cleared queues");

    // Enqueue a successful job
    let job = DebugJob::new("debug_job_1", true);
    let job_id = queue.enqueue(job).await?;
    println!("🔍 DEBUG: Enqueued job with ID: {}", job_id);

    // Check queue size
    let size = queue.retry_queue_size().await?;
    println!("🔍 DEBUG: Queue size after enqueue: {}", size);

    // Process jobs for a short time
    println!("🔍 DEBUG: Starting worker...");
    let worker_task = tokio::spawn(async move {
        queue.start_worker().await
    });

    // Give worker time to process
    println!("🔍 DEBUG: Waiting for job processing...");
    time::sleep(Duration::from_millis(500)).await;
    
    println!("🔍 DEBUG: Aborting worker...");
    worker_task.abort();

    // Check that job was executed
    let executions = counter.lock().unwrap();
    println!("🔍 DEBUG: Final execution count: {}", executions.len());
    println!("🔍 DEBUG: Executed jobs: {:?}", *executions);

    Ok(())
}
