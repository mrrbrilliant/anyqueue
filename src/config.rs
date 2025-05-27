use std::{env, time::Duration};

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

    /// Load configuration from environment variables
    pub fn from_env(mut self) -> Self {
        self.config = AnyQueueConfig::from_env();
        self
    }

    /// Validate and build the final configuration
    pub fn build_and_validate(self) -> Result<AnyQueueConfig, String> {
        self.config.validate()?;
        Ok(self.config)
    }

    /// Build the final configuration
    pub fn build(self) -> AnyQueueConfig {
        self.config
    }
}

impl AnyQueueConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = AnyQueueConfig::default();

        if let Ok(redis_url) = env::var("ANYQUEUE_REDIS_URL") {
            config.redis_url = redis_url;
        }

        if let Ok(max_retries) = env::var("ANYQUEUE_MAX_RETRIES") {
            if let Ok(retries) = max_retries.parse::<u32>() {
                config.max_retries = retries;
            }
        }

        if let Ok(base_delay) = env::var("ANYQUEUE_BASE_DELAY_SECS") {
            if let Ok(delay_secs) = base_delay.parse::<u64>() {
                config.base_delay = Duration::from_secs(delay_secs);
            }
        }

        if let Ok(max_delay) = env::var("ANYQUEUE_MAX_DELAY_SECS") {
            if let Ok(delay_secs) = max_delay.parse::<u64>() {
                config.max_delay = Duration::from_secs(delay_secs);
            }
        }

        if let Ok(job_limit) = env::var("ANYQUEUE_JOB_PROCESSING_LIMIT") {
            if let Ok(limit) = job_limit.parse::<usize>() {
                config.job_processing_limit = limit;
            }
        }

        if let Ok(poll_interval) = env::var("ANYQUEUE_WORKER_POLL_INTERVAL_SECS") {
            if let Ok(interval_secs) = poll_interval.parse::<u64>() {
                config.worker_poll_interval = Duration::from_secs(interval_secs);
            }
        }

        if let Ok(max_errors) = env::var("ANYQUEUE_MAX_CONSECUTIVE_ERRORS") {
            if let Ok(errors) = max_errors.parse::<u32>() {
                config.max_consecutive_errors = errors;
            }
        }

        config
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.redis_url.is_empty() {
            return Err("Redis URL cannot be empty".to_string());
        }

        if self.max_retries == 0 {
            return Err("Max retries must be greater than 0".to_string());
        }

        if self.base_delay.is_zero() {
            return Err("Base delay must be greater than 0".to_string());
        }

        if self.max_delay < self.base_delay {
            return Err("Max delay must be greater than or equal to base delay".to_string());
        }

        if self.job_processing_limit == 0 {
            return Err("Job processing limit must be greater than 0".to_string());
        }

        if self.worker_poll_interval.is_zero() {
            return Err("Worker poll interval must be greater than 0".to_string());
        }

        if self.max_consecutive_errors == 0 {
            return Err("Max consecutive errors must be greater than 0".to_string());
        }

        Ok(())
    }
}
