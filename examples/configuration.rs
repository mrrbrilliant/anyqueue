use anyqueue::{AnyQueue, AnyQueueConfig, Job, GenericJobProcessor, Result};
use serde::{Deserialize, Serialize};
use std::{env, time::Duration};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ConfigurableJob {
    id: String,
    message: String,
}

#[async_trait::async_trait]
impl Job for ConfigurableJob {
    async fn process(&self) -> Result<()> {
        println!("📋 Processing configurable job: {} - {}", self.id, self.message);
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(())
    }
}

fn demonstrate_config_options() {
    println!("🔧 Configuration Options:");
    println!("========================");
    println!("Environment Variables:");
    println!("  ANYQUEUE_REDIS_URL - Redis connection URL");
    println!("  ANYQUEUE_MAX_RETRIES - Maximum retry attempts");
    println!("  ANYQUEUE_BASE_DELAY_SECS - Base delay in seconds");
    println!("  ANYQUEUE_MAX_DELAY_SECS - Maximum delay cap in seconds");
    println!("  ANYQUEUE_JOB_PROCESSING_LIMIT - Jobs to process at once");
    println!("  ANYQUEUE_WORKER_POLL_INTERVAL_SECS - Worker polling interval");
    println!("  ANYQUEUE_MAX_CONSECUTIVE_ERRORS - Max Redis errors before stopping");
    println!();
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("⚙️  AnyQueue Configuration Demo");
    println!("==============================");

    demonstrate_config_options();

    // Method 1: Load configuration from environment variables
    println!("🔄 Method 1: Loading configuration from environment...");
    let env_config = AnyQueueConfig::from_env();
    
    match env_config.validate() {
        Ok(()) => {
            println!("✅ Environment configuration is valid");
            println!("   Redis URL: {}", env_config.redis_url);
            println!("   Max Retries: {}", env_config.max_retries);
            println!("   Base Delay: {:?}", env_config.base_delay);
        }
        Err(e) => {
            println!("❌ Environment configuration validation failed: {}", e);
            println!("   Using defaults instead...");
        }
    }

    // Method 2: Builder pattern with environment override
    println!("\n🔄 Method 2: Builder pattern with environment override...");
    let queue_result = AnyQueue::builder()
        .from_env() // Load from environment first
        .redis_url("redis://127.0.0.1:6379") // Override if needed
        .max_retries(3)
        .base_delay(Duration::from_secs(1))
        .processor(GenericJobProcessor::<ConfigurableJob>::new())
        .build()
        .await;

    let mut queue = match queue_result {
        Ok(q) => {
            println!("✅ Queue created successfully with configuration");
            q
        }
        Err(e) => {
            println!("❌ Failed to create queue: {}", e);
            return Err(e);
        }
    };

    // Method 3: Manual configuration with validation
    println!("\n🔄 Method 3: Manual configuration with validation...");
    let manual_config = AnyQueueConfig {
        redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
        retry_queue_key: "demo:retry".to_string(),
        dead_letter_queue_key: "demo:dlq".to_string(),
        max_retries: 5,
        base_delay: Duration::from_secs(2),
        max_delay: Duration::from_secs(60),
        job_processing_limit: 10,
        worker_poll_interval: Duration::from_secs(1),
        max_consecutive_errors: 3,
    };

    match manual_config.validate() {
        Ok(()) => println!("✅ Manual configuration is valid"),
        Err(e) => println!("❌ Manual configuration validation failed: {}", e),
    }

    // Clear queues for demo
    queue.clear_all_queues().await?;
    println!("\n🧹 Cleared existing queues");

    // Add some test jobs
    for i in 1..=5 {
        let job = ConfigurableJob {
            id: format!("config_job_{}", i),
            message: format!("Job {} with current configuration", i),
        };
        queue.enqueue(job).await?;
    }

    println!("📝 Enqueued 5 test jobs");
    println!("📊 Current queue stats:");
    
    let stats = queue.get_stats().await?;
    println!("   Retry queue: {} jobs", stats.retry_queue_size);
    println!("   Dead letter queue: {} jobs", stats.dead_letter_queue_size);
    println!("   Jobs due now: {}", stats.estimated_jobs_due_now);

    // Test health check
    let health = queue.health_check().await;
    println!("\n🏥 Health check:");
    println!("   Redis connected: {}", health.redis_connected);
    println!("   Last check: {}", chrono::DateTime::from_timestamp_millis(health.last_check_timestamp).unwrap_or_default());

    println!("\n✅ Configuration demo complete!");
    println!("To test with custom config, try:");
    println!("export ANYQUEUE_MAX_RETRIES=10");
    println!("export ANYQUEUE_BASE_DELAY_SECS=5");
    println!("cargo run --example configuration");

    Ok(())
}
