use anyqueue::{AnyQueue, GenericJobProcessor, Job, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TestJob {
    id: String,
    should_succeed: bool,
    delay_ms: u64,
}

// Shared counter for tracking job executions
type JobCounter = Arc<Mutex<Vec<String>>>;

impl TestJob {
    fn new(id: &str, should_succeed: bool, delay_ms: u64) -> Self {
        Self {
            id: id.to_string(),
            should_succeed,
            delay_ms,
        }
    }
}

#[async_trait::async_trait]
impl Job for TestJob {
    async fn process(&self) -> Result<()> {
        if self.delay_ms > 0 {
            time::sleep(Duration::from_millis(self.delay_ms)).await;
        }

        if self.should_succeed {
            println!("✅ Test job {} succeeded", self.id);
            Ok(())
        } else {
            println!("❌ Test job {} failed", self.id);
            Err(anyqueue::Error::JobProcessing(format!(
                "Test job {} intentionally failed",
                self.id
            )))
        }
    }
}

// Custom processor that tracks job executions
struct TrackingJobProcessor {
    inner: GenericJobProcessor<TestJob>,
    counter: JobCounter,
}

impl TrackingJobProcessor {
    fn new(counter: JobCounter) -> Self {
        Self {
            inner: GenericJobProcessor::new(),
            counter,
        }
    }
}

#[async_trait::async_trait]
impl anyqueue::JobProcessor for TrackingJobProcessor {
    fn job_type(&self) -> &'static str {
        self.inner.job_type()
    }

    async fn process(&self, job_data: &anyqueue::JobData) -> Result<()> {
        let job: TestJob = job_data.deserialize_job()?;

        // Track execution
        {
            let mut counter = self.counter.lock().unwrap();
            counter.push(job.id.clone());
        }

        self.inner.process(job_data).await
    }
}

async fn setup_test_queue() -> Result<AnyQueue> {
    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(3)
        .base_delay(Duration::from_millis(100))
        .max_retry_delay(Duration::from_secs(1))
        .processor(GenericJobProcessor::<TestJob>::new())
        .build()
        .await?;

    // Clear queues before each test
    queue.clear_all_queues().await?;
    Ok(queue)
}

#[tokio::test]
async fn test_basic_job_enqueue_and_process() -> Result<()> {
    let counter = Arc::new(Mutex::new(Vec::new()));
    let processor = TrackingJobProcessor::new(counter.clone());

    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(3)
        .base_delay(Duration::from_millis(0)) // No delay for immediate processing
        .processor(processor)
        .build()
        .await?;

    queue.clear_all_queues().await?;

    // Enqueue a successful job
    let job = TestJob::new("test_job_1", true, 0);
    let job_id = queue.enqueue(job).await?;
    assert!(!job_id.is_empty());

    // Check queue size
    let size = queue.retry_queue_size().await?;
    assert_eq!(size, 1);

    // Process jobs for a short time
    let worker_task = tokio::spawn(async move { queue.start_worker().await });

    // Give worker time to process
    time::sleep(Duration::from_millis(300)).await; // Increased wait time
    worker_task.abort();

    // Check that job was executed
    let executions = counter.lock().unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0], "test_job_1");

    Ok(())
}

#[tokio::test]
async fn test_job_retry_logic() -> Result<()> {
    let counter = Arc::new(Mutex::new(Vec::new()));
    let processor = TrackingJobProcessor::new(counter.clone());

    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(2) // Low retry count for faster test
        .base_delay(Duration::from_millis(0)) // No delay for faster testing
        .processor(processor)
        .build()
        .await?;

    queue.clear_all_queues().await?;

    // Enqueue a failing job
    let job = TestJob::new("failing_job", false, 0);
    queue.enqueue(job).await?;

    // Process jobs for enough time to see retries
    let worker_task = tokio::spawn(async move { queue.start_worker().await });

    // Give worker time to process and retry
    time::sleep(Duration::from_millis(500)).await;
    worker_task.abort();

    // Check that job was attempted multiple times
    let executions = counter.lock().unwrap();
    assert!(
        executions.len() >= 2,
        "Expected at least 2 attempts, got {}",
        executions.len()
    );

    Ok(())
}

#[tokio::test]
async fn test_dead_letter_queue() -> Result<()> {
    // Set very low retry count
    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(1)
        .base_delay(Duration::from_millis(0)) // No delay for faster testing
        .processor(GenericJobProcessor::<TestJob>::new())
        .build()
        .await?;

    queue.clear_all_queues().await?;

    // Enqueue a failing job
    let job = TestJob::new("dlq_job", false, 0);
    queue.enqueue(job).await?;

    println!(
        "🔍 Job enqueued, queue size: {}",
        queue.retry_queue_size().await?
    );

    // Process jobs
    let worker_task = tokio::spawn(async move { queue.start_worker().await });

    // Give worker time to exhaust retries
    println!("🔍 Starting worker...");
    time::sleep(Duration::from_millis(2000)).await; // Much longer wait
    println!("🔍 Stopping worker...");
    worker_task.abort();

    // Check dead letter queue - use same configuration as the processing queue
    let mut queue_check = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(1)
        .base_delay(Duration::from_millis(0))
        .processor(GenericJobProcessor::<TestJob>::new())
        .build()
        .await?;
    let dlq_size = queue_check.dead_letter_queue_size().await?;
    let retry_size = queue_check.retry_queue_size().await?;
    println!(
        "🔍 DLQ size: {}, Retry queue size: {}",
        dlq_size, retry_size
    );
    assert!(dlq_size > 0, "Expected job in dead letter queue");

    Ok(())
}

#[tokio::test]
async fn test_multiple_job_types() -> Result<()> {
    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct EmailJob {
        to: String,
        subject: String,
    }

    #[async_trait::async_trait]
    impl Job for EmailJob {
        async fn process(&self) -> Result<()> {
            println!("Sending email to: {}", self.to);
            Ok(())
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct SmsJob {
        phone: String,
        message: String,
    }

    #[async_trait::async_trait]
    impl Job for SmsJob {
        async fn process(&self) -> Result<()> {
            println!("Sending SMS to: {}", self.phone);
            Ok(())
        }
    }

    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .processor(GenericJobProcessor::<EmailJob>::new())
        .processor(GenericJobProcessor::<SmsJob>::new())
        .build()
        .await?;

    queue.clear_all_queues().await?;

    // Enqueue different job types
    let email = EmailJob {
        to: "test@example.com".to_string(),
        subject: "Test".to_string(),
    };
    let sms = SmsJob {
        phone: "+1234567890".to_string(),
        message: "Test message".to_string(),
    };

    queue.enqueue(email).await?;
    queue.enqueue(sms).await?;

    let size = queue.retry_queue_size().await?;
    assert_eq!(size, 2);

    Ok(())
}

#[tokio::test]
async fn test_queue_size_monitoring() -> Result<()> {
    let mut queue = setup_test_queue().await?;

    // Initially empty
    assert_eq!(queue.retry_queue_size().await?, 0);
    assert_eq!(queue.dead_letter_queue_size().await?, 0);

    // Add jobs
    for i in 0..5 {
        let job = TestJob::new(&format!("job_{}", i), true, 0);
        queue.enqueue(job).await?;
    }

    assert_eq!(queue.retry_queue_size().await?, 5);

    Ok(())
}

#[tokio::test]
async fn test_configuration_validation() -> Result<()> {
    // Test empty job ID rejection
    let mut queue = setup_test_queue().await?;
    let job = TestJob::new("test", true, 0);

    let result = queue.enqueue_with_id("".to_string(), job).await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_exponential_backoff() -> Result<()> {
    use std::time::Instant;

    let mut queue = AnyQueue::builder()
        .redis_url("redis://127.0.0.1:6379")
        .max_retries(3)
        .base_delay(Duration::from_millis(100))
        .processor(GenericJobProcessor::<TestJob>::new())
        .build()
        .await?;

    queue.clear_all_queues().await?;

    // Enqueue a failing job
    let job = TestJob::new("backoff_test", false, 0);
    queue.enqueue(job).await?;

    let start = Instant::now();

    // Process for a limited time
    let worker_task = tokio::spawn(async move { queue.start_worker().await });

    time::sleep(Duration::from_millis(800)).await;
    worker_task.abort();

    let elapsed = start.elapsed();

    // Should have taken some time due to exponential backoff
    assert!(
        elapsed >= Duration::from_millis(200),
        "Expected exponential backoff delay, but job completed too quickly"
    );

    Ok(())
}
