use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// Trait that all jobs must implement
#[async_trait]
pub trait Job: Send + Sync + Debug {
    /// Process the job. Return Ok(()) if successful, Err if it should be retried
    async fn process(&self) -> Result<()>;

    /// Optional: Get a unique identifier for this job type
    fn job_type(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Internal job data structure used by the queue
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JobData {
    pub id: String,
    pub job_type: String,
    pub data: serde_json::Value,
    pub retries: u32,
    pub created_at: i64,
    pub last_retry_at: Option<i64>,
}

impl JobData {
    pub fn new<T>(id: String, job: &T) -> Result<Self>
    where
        T: Job + Serialize,
    {
        let data = serde_json::to_value(job)?;
        let now = chrono::Utc::now().timestamp_millis();

        Ok(Self {
            id,
            job_type: job.job_type().to_string(),
            data,
            retries: 0,
            created_at: now,
            last_retry_at: None,
        })
    }

    pub fn deserialize_job<T>(&self) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        Ok(serde_json::from_value(self.data.clone())?)
    }
}

/// Trait for processing jobs of a specific type
#[async_trait]
pub trait JobProcessor: Send + Sync {
    /// The job type this processor handles
    fn job_type(&self) -> &'static str;

    /// Process a job
    async fn process(&self, job_data: &JobData) -> Result<()>;
}

/// A processor that can handle any job implementing the Job trait
pub struct GenericJobProcessor<T>
where
    T: Job + for<'de> Deserialize<'de>,
{
    _phantom: std::marker::PhantomData<T>,
}

impl<T> GenericJobProcessor<T>
where
    T: Job + for<'de> Deserialize<'de>,
{
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Default for GenericJobProcessor<T>
where
    T: Job + for<'de> Deserialize<'de>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T> JobProcessor for GenericJobProcessor<T>
where
    T: Job + for<'de> Deserialize<'de>,
{
    fn job_type(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    async fn process(&self, job_data: &JobData) -> Result<()> {
        let job: T = job_data.deserialize_job()?;
        job.process().await
    }
}
