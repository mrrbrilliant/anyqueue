use std::time::Duration;

/// Configuration for AnyQueue
#[derive(Debug, Clone)]
pub struct AnyQueueConfig {
    pub redis_url: String,
    pub retry_queue_key: String,
    pub dead_letter_queue_key: String,
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub job_processing_limit: usize,
    pub worker_poll_interval: Duration,
    pub max_consecutive_errors: u32,
}

impl Default for AnyQueueConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            retry_queue_key: "anyqueue:retry".to_string(),
            dead_letter_queue_key: "anyqueue:dlq".to_string(),
            max_retries: 5,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(300), // 5 minutes
            job_processing_limit: 5,
            worker_poll_interval: Duration::from_secs(1),
            max_consecutive_errors: 5,
        }
    }
}

/// Builder for AnyQueueConfig
#[derive(Debug)]
pub struct AnyQueueConfigBuilder {
    config: AnyQueueConfig,
}

impl Default for AnyQueueConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AnyQueueConfigBuilder {
    /// Create a new config builder with default values
    pub fn new() -> Self {
        Self {
            config: AnyQueueConfig::default(),
        }
    }

    /// Set the Redis URL
    pub fn redis_url<S: Into<String>>(mut self, url: S) -> Self {
        self.config.redis_url = url.into();
        self
    }

    /// Set the retry queue key
    pub fn retry_queue_key<S: Into<String>>(mut self, key: S) -> Self {
        self.config.retry_queue_key = key.into();
        self
    }

    /// Set the dead letter queue key
    pub fn dead_letter_queue_key<S: Into<String>>(mut self, key: S) -> Self {
        self.config.dead_letter_queue_key = key.into();
        self
    }

    /// Set the maximum number of retries
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    /// Set the base delay for exponential backoff
    pub fn base_delay(mut self, delay: Duration) -> Self {
        self.config.base_delay = delay;
        self
    }

    /// Set the maximum delay cap
    pub fn max_delay(mut self, delay: Duration) -> Self {
        self.config.max_delay = delay;
        self
    }

    /// Set the number of jobs to process at once
    pub fn job_processing_limit(mut self, limit: usize) -> Self {
        self.config.job_processing_limit = limit;
        self
    }

    /// Set the worker polling interval
    pub fn worker_poll_interval(mut self, interval: Duration) -> Self {
        self.config.worker_poll_interval = interval;
        self
    }

    /// Set the maximum consecutive errors before worker stops
    pub fn max_consecutive_errors(mut self, max_errors: u32) -> Self {
        self.config.max_consecutive_errors = max_errors;
        self
    }

    /// Build the final configuration
    pub fn build(self) -> AnyQueueConfig {
        self.config
    }
}
