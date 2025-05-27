use crate::{
    config::AnyQueueConfig,
    error::{Error, Result},
    job::{JobData, JobProcessor},
    worker::Worker,
};
use redis::{aio::MultiplexedConnection, AsyncCommands, Client};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};

/// Health status of the queue
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub redis_connected: bool,
    pub retry_queue_size: usize,
    pub dead_letter_queue_size: usize,
    pub last_check_timestamp: i64,
}

/// Queue statistics for monitoring
#[derive(Debug, Clone, Serialize)]
pub struct QueueStats {
    pub retry_queue_size: usize,
    pub dead_letter_queue_size: usize,
    pub estimated_jobs_due_now: usize,
    pub total_jobs_processed: Option<u64>, // Future enhancement
}

/// Main queue interface
pub struct AnyQueue {
    config: AnyQueueConfig,
    redis_client: Client,
    connection: MultiplexedConnection,
    processors: Arc<HashMap<String, Box<dyn JobProcessor>>>,
}

impl AnyQueue {
    /// Create a new AnyQueue builder
    pub fn builder() -> AnyQueueBuilder {
        AnyQueueBuilder::new()
    }

    /// Create a new AnyQueue with default configuration
    pub async fn new() -> Result<Self> {
        Self::with_config(AnyQueueConfig::default()).await
    }

    /// Create a new AnyQueue with custom configuration
    pub async fn with_config(config: AnyQueueConfig) -> Result<Self> {
        let redis_client = Client::open(config.redis_url.as_str())
            .map_err(|e| Error::Connection(format!("Failed to create Redis client: {}", e)))?;

        let connection = redis_client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| Error::Connection(format!("Failed to connect to Redis: {}", e)))?;

        Ok(Self {
            config,
            redis_client,
            connection,
            processors: Arc::new(HashMap::new()),
        })
    }

    /// Enqueue a job with automatic ID generation
    pub async fn enqueue<T>(&mut self, job: T) -> Result<String>
    where
        T: crate::job::Job + Serialize,
    {
        let job_id = uuid::Uuid::new_v4().to_string();
        self.enqueue_with_id(job_id.clone(), job).await?;
        Ok(job_id)
    }

    /// Enqueue a job with a specific ID
    pub async fn enqueue_with_id<T>(&mut self, job_id: String, job: T) -> Result<()>
    where
        T: crate::job::Job + Serialize,
    {
        if job_id.is_empty() {
            return Err(Error::Config("Job ID cannot be empty".to_string()));
        }

        let job_data = JobData::new(job_id, &job)?;
        self.add_job_to_retry_queue(job_data).await
    }

    /// Start the worker to process jobs
    pub async fn start_worker(&self) -> Result<()> {
        let worker = Worker::new(
            self.config.clone(),
            self.redis_client.clone(),
            self.processors.clone(),
        )
        .await?;

        worker.start().await
    }

    /// Clear all queues (useful for testing)
    pub async fn clear_all_queues(&mut self) -> Result<()> {
        let _: () = redis::cmd("DEL")
            .arg(&self.config.retry_queue_key)
            .exec_async(&mut self.connection)
            .await?;

        let _: () = redis::cmd("DEL")
            .arg(&self.config.dead_letter_queue_key)
            .exec_async(&mut self.connection)
            .await?;

        Ok(())
    }

    /// Get the number of jobs in the retry queue
    pub async fn retry_queue_size(&mut self) -> Result<usize> {
        let size: usize = self.connection.zcard(&self.config.retry_queue_key).await?;
        Ok(size)
    }

    /// Get the number of jobs in the dead letter queue
    pub async fn dead_letter_queue_size(&mut self) -> Result<usize> {
        let size: usize = self
            .connection
            .llen(&self.config.dead_letter_queue_key)
            .await?;
        Ok(size)
    }

    /// Get queue statistics for monitoring
    pub async fn get_stats(&mut self) -> Result<QueueStats> {
        let retry_queue_size = self.retry_queue_size().await?;
        let dead_letter_queue_size = self.dead_letter_queue_size().await?;
        
        // Count jobs that are due for processing now
        let now = chrono::Utc::now().timestamp_millis();
        let estimated_jobs_due_now: usize = self
            .connection
            .zcount(&self.config.retry_queue_key, 0, now)
            .await?;
        
        Ok(QueueStats {
            retry_queue_size,
            dead_letter_queue_size,
            estimated_jobs_due_now,
            total_jobs_processed: None, // Future enhancement with Redis counters
        })
    }

    /// Check health status of the queue system
    pub async fn health_check(&mut self) -> HealthStatus {
        let now = chrono::Utc::now().timestamp_millis();
        
        // Test Redis connectivity by attempting a simple operation
        let redis_connected = match redis::cmd("PING")
            .query_async::<_, String>(&mut self.connection)
            .await 
        {
            Ok(_) => true,
            Err(_) => false,
        };
        
        let (retry_queue_size, dead_letter_queue_size) = if redis_connected {
            (
                self.retry_queue_size().await.unwrap_or(0),
                self.dead_letter_queue_size().await.unwrap_or(0),
            )
        } else {
            (0, 0)
        };
        
        HealthStatus {
            redis_connected,
            retry_queue_size,
            dead_letter_queue_size,
            last_check_timestamp: now,
        }
    }

    async fn add_job_to_retry_queue(&mut self, job_data: JobData) -> Result<()> {
        let initial_delay_ms = self.config.base_delay.as_millis() as i64;
        let next_retry_timestamp_ms = chrono::Utc::now().timestamp_millis() + initial_delay_ms;

        let job_json = serde_json::to_string(&job_data)?;

        log::info!(
            "Adding job {} to retry queue with initial delay of {:?}",
            job_data.id,
            self.config.base_delay
        );

        let _: () = redis::cmd("ZADD")
            .arg(&self.config.retry_queue_key)
            .arg(next_retry_timestamp_ms)
            .arg(job_json)
            .exec_async(&mut self.connection)
            .await?;

        Ok(())
    }
}

/// Builder for AnyQueue
pub struct AnyQueueBuilder {
    config: crate::config::AnyQueueConfigBuilder,
    processors: HashMap<String, Box<dyn JobProcessor>>,
}

impl AnyQueueBuilder {
    pub fn new() -> Self {
        Self {
            config: crate::config::AnyQueueConfigBuilder::new(),
            processors: HashMap::new(),
        }
    }

    /// Set the Redis URL
    pub fn redis_url<S: Into<String>>(mut self, url: S) -> Self {
        self.config = self.config.redis_url(url);
        self
    }

    /// Set the maximum number of retries
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.config = self.config.max_retries(max_retries);
        self
    }

    /// Set the base delay for exponential backoff
    pub fn base_delay(mut self, delay: std::time::Duration) -> Self {
        self.config = self.config.base_delay(delay);
        self
    }

    /// Set the maximum retry delay cap
    pub fn max_retry_delay(mut self, delay: std::time::Duration) -> Self {
        self.config = self.config.max_delay(delay);
        self
    }

    /// Add a job processor
    pub fn processor<P>(mut self, processor: P) -> Self
    where
        P: JobProcessor + 'static,
    {
        let job_type = processor.job_type().to_string();
        self.processors.insert(job_type, Box::new(processor));
        self
    }

    /// Build the AnyQueue
    pub async fn build(self) -> Result<AnyQueue> {
        let config = self.config.build();
        let mut queue = AnyQueue::with_config(config).await?;
        queue.processors = Arc::new(self.processors);
        Ok(queue)
    }
}

impl Default for AnyQueueBuilder {
    fn default() -> Self {
        Self::new()
    }
}
