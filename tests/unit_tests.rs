use anyqueue::{AnyQueueConfig, Error, JobProcessor, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[test]
fn test_config_builder() {
    let config = anyqueue::AnyQueueConfigBuilder::new()
        .redis_url("redis://test:6379")
        .max_retries(10)
        .base_delay(Duration::from_secs(5))
        .max_delay(Duration::from_secs(600))
        .job_processing_limit(20)
        .worker_poll_interval(Duration::from_millis(500))
        .max_consecutive_errors(15)
        .build();

    assert_eq!(config.redis_url, "redis://test:6379");
    assert_eq!(config.max_retries, 10);
    assert_eq!(config.base_delay, Duration::from_secs(5));
    assert_eq!(config.max_delay, Duration::from_secs(600));
    assert_eq!(config.job_processing_limit, 20);
    assert_eq!(config.worker_poll_interval, Duration::from_millis(500));
    assert_eq!(config.max_consecutive_errors, 15);
}

#[test]
fn test_config_defaults() {
    let config = AnyQueueConfig::default();

    assert_eq!(config.redis_url, "redis://127.0.0.1:6379");
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.base_delay, Duration::from_secs(2));
    assert_eq!(config.max_delay, Duration::from_secs(300));
    assert_eq!(config.job_processing_limit, 5);
    assert_eq!(config.worker_poll_interval, Duration::from_secs(1));
    assert_eq!(config.max_consecutive_errors, 5);
}

#[test]
fn test_error_display() {
    let config_error = Error::Config("Invalid configuration".to_string());
    assert_eq!(
        config_error.to_string(),
        "Configuration error: Invalid configuration"
    );

    let job_error = Error::JobProcessing("Job failed".to_string());
    assert_eq!(job_error.to_string(), "Job processing error: Job failed");

    let worker_error = Error::Worker("Worker stopped".to_string());
    assert_eq!(worker_error.to_string(), "Worker error: Worker stopped");

    let connection_error = Error::Connection("Redis down".to_string());
    assert_eq!(connection_error.to_string(), "Connection error: Redis down");
}

#[test]
fn test_error_from_serde() {
    let serde_error = serde_json::from_str::<String>("invalid json").unwrap_err();
    let anyqueue_error: Error = serde_error.into();

    match anyqueue_error {
        Error::Serialization(_) => {} // Expected
        _ => panic!("Expected Serialization error"),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TestJobData {
    id: String,
    data: String,
}

#[async_trait::async_trait]
impl anyqueue::Job for TestJobData {
    async fn process(&self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn test_job_data_creation() {
    let job = TestJobData {
        id: "test_123".to_string(),
        data: "test_data".to_string(),
    };

    let job_data = anyqueue::JobData::new("job_123".to_string(), &job).unwrap();

    assert_eq!(job_data.id, "job_123");
    assert_eq!(job_data.job_type, std::any::type_name::<TestJobData>());
    assert_eq!(job_data.retries, 0);
    assert!(job_data.last_retry_at.is_none());

    // Test deserialization
    let deserialized: TestJobData = job_data.deserialize_job().unwrap();
    assert_eq!(deserialized.id, "test_123");
    assert_eq!(deserialized.data, "test_data");
}

#[test]
fn test_generic_job_processor() {
    let processor = anyqueue::GenericJobProcessor::<TestJobData>::new();
    assert_eq!(processor.job_type(), std::any::type_name::<TestJobData>());
}

#[tokio::test]
async fn test_job_data_serialization_roundtrip() {
    let job = TestJobData {
        id: "roundtrip_test".to_string(),
        data: "some data".to_string(),
    };

    let job_data = anyqueue::JobData::new("test_job".to_string(), &job).unwrap();

    // Serialize to JSON
    let json = serde_json::to_string(&job_data).unwrap();

    // Deserialize back
    let deserialized_job_data: anyqueue::JobData = serde_json::from_str(&json).unwrap();

    assert_eq!(job_data.id, deserialized_job_data.id);
    assert_eq!(job_data.job_type, deserialized_job_data.job_type);
    assert_eq!(job_data.retries, deserialized_job_data.retries);

    // Test job deserialization
    let original_job: TestJobData = deserialized_job_data.deserialize_job().unwrap();
    assert_eq!(original_job.id, "roundtrip_test");
    assert_eq!(original_job.data, "some data");
}
