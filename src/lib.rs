//! # AnyQueue - Redis-based Retry Queue Library
//!
//! A robust, asynchronous job processing library with automatic retry logic,
//! exponential backoff, and dead letter queue functionality.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use anyqueue::{AnyQueue, Job};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, Debug)]
//! struct EmailJob {
//!     to: String,
//!     subject: String,
//!     body: String,
//! }
//!
//! impl Job for EmailJob {
//!     async fn process(&self) -> anyqueue::Result<()> {
//!         // Send email logic here
//!         println!("Sending email to: {}", self.to);
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyqueue::Result<()> {
//!     let queue = AnyQueue::builder()
//!         .redis_url("redis://localhost:6379")
//!         .max_retries(3)
//!         .build()
//!         .await?;
//!
//!     // Add a job
//!     let email_job = EmailJob {
//!         to: "user@example.com".to_string(),
//!         subject: "Hello".to_string(),
//!         body: "World!".to_string(),
//!     };
//!
//!     queue.enqueue("email_job_1", email_job).await?;
//!
//!     // Start processing
//!     queue.start_worker().await?;
//!
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod error;
pub mod job;
pub mod queue;
pub mod worker;

pub use config::{AnyQueueConfig, AnyQueueConfigBuilder};
pub use error::{Error, Result};
pub use job::{GenericJobProcessor, Job, JobData, JobProcessor};
pub use queue::AnyQueue;
pub use worker::Worker;

// Re-export commonly used types
pub use serde_json::Value as JsonValue;
