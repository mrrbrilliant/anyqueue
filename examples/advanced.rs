use anyqueue::{AnyQueue, Job, JobData, JobProcessor, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Different job types
#[derive(Serialize, Deserialize, Debug, Clone)]
struct EmailJob {
    to: String,
    subject: String,
    body: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct PaymentJob {
    user_id: u64,
    amount: f64,
    currency: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ReportJob {
    report_type: String,
    start_date: String,
    end_date: String,
}

// Implement Job trait for each type
#[async_trait::async_trait]
impl Job for EmailJob {
    async fn process(&self) -> Result<()> {
        println!("📧 Processing email to: {}", self.to);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        if rand::random::<f64>() < 0.8 {
            Ok(())
        } else {
            Err(anyqueue::Error::JobProcessing(
                "SMTP server timeout".to_string(),
            ))
        }
    }
}

#[async_trait::async_trait]
impl Job for PaymentJob {
    async fn process(&self) -> Result<()> {
        println!(
            "💳 Processing payment: {} {} for user {}",
            self.amount, self.currency, self.user_id
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        if rand::random::<f64>() < 0.6 {
            Ok(())
        } else {
            Err(anyqueue::Error::JobProcessing(
                "Payment gateway error".to_string(),
            ))
        }
    }
}

#[async_trait::async_trait]
impl Job for ReportJob {
    async fn process(&self) -> Result<()> {
        println!(
            "📊 Generating {} report from {} to {}",
            self.report_type, self.start_date, self.end_date
        );
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        if rand::random::<f64>() < 0.9 {
            Ok(())
        } else {
            Err(anyqueue::Error::JobProcessing(
                "Database connection lost".to_string(),
            ))
        }
    }
}

// Custom job processor with metrics
struct MetricsJobProcessor<T>
where
    T: Job + for<'de> Deserialize<'de>,
{
    inner: anyqueue::GenericJobProcessor<T>,
    metrics: std::sync::Arc<std::sync::Mutex<HashMap<String, u64>>>,
}

impl<T> MetricsJobProcessor<T>
where
    T: Job + for<'de> Deserialize<'de>,
{
    fn new() -> Self {
        Self {
            inner: anyqueue::GenericJobProcessor::new(),
            metrics: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn get_metrics(&self) -> HashMap<String, u64> {
        self.metrics.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl<T> JobProcessor for MetricsJobProcessor<T>
where
    T: Job + for<'de> Deserialize<'de>,
{
    fn job_type(&self) -> &'static str {
        self.inner.job_type()
    }

    async fn process(&self, job_data: &JobData) -> Result<()> {
        let start = std::time::Instant::now();
        let result = self.inner.process(job_data).await;
        let duration = start.elapsed();

        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            let counter_key = format!("{}_total", self.job_type());
            let duration_key = format!("{}_duration_ms", self.job_type());

            *metrics.entry(counter_key).or_insert(0) += 1;
            *metrics.entry(duration_key).or_insert(0) += duration.as_millis() as u64;

            if result.is_ok() {
                let success_key = format!("{}_success", self.job_type());
                *metrics.entry(success_key).or_insert(0) += 1;
            } else {
                let failure_key = format!("{}_failure", self.job_type());
                *metrics.entry(failure_key).or_insert(0) += 1;
            }
        }

        result
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Create processors with metrics
    let email_processor = MetricsJobProcessor::<EmailJob>::new();
    let payment_processor = MetricsJobProcessor::<PaymentJob>::new();
    let report_processor = MetricsJobProcessor::<ReportJob>::new();

    // Get references to metrics for later reporting
    let email_metrics = email_processor.metrics.clone();
    let payment_metrics = payment_processor.metrics.clone();
    let report_metrics = report_processor.metrics.clone();

    // Build queue with custom processors
    let mut queue = AnyQueue::builder()
        .redis_url("redis://localhost:6379")
        .max_retries(5)
        .base_delay(std::time::Duration::from_secs(2))
        .max_retry_delay(std::time::Duration::from_secs(60))
        .processor(email_processor)
        .processor(payment_processor)
        .processor(report_processor)
        .build()
        .await?;

    // Clear existing jobs
    queue.clear_all_queues().await?;

    // Add various types of jobs
    let jobs: Vec<Box<dyn Job + Send + Sync>> = vec![
        Box::new(EmailJob {
            to: "user1@example.com".to_string(),
            subject: "Welcome".to_string(),
            body: "Welcome to our platform!".to_string(),
        }),
        Box::new(PaymentJob {
            user_id: 123,
            amount: 99.99,
            currency: "USD".to_string(),
        }),
        Box::new(ReportJob {
            report_type: "monthly_sales".to_string(),
            start_date: "2024-01-01".to_string(),
            end_date: "2024-01-31".to_string(),
        }),
        Box::new(EmailJob {
            to: "user2@example.com".to_string(),
            subject: "Newsletter".to_string(),
            body: "Check out our latest features!".to_string(),
        }),
        Box::new(PaymentJob {
            user_id: 456,
            amount: 149.99,
            currency: "EUR".to_string(),
        }),
    ];

    // Note: This is a simplified example. In practice, you'd need to handle
    // the trait object serialization differently or use an enum-based approach
    println!("📝 Would enqueue {} jobs of different types", jobs.len());

    // For this example, let's add jobs individually
    queue
        .enqueue(EmailJob {
            to: "test@example.com".to_string(),
            subject: "Test".to_string(),
            body: "Test message".to_string(),
        })
        .await?;

    queue
        .enqueue(PaymentJob {
            user_id: 789,
            amount: 25.50,
            currency: "GBP".to_string(),
        })
        .await?;

    // Start a metrics reporting task
    let metrics_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;

            println!("\n📊 === METRICS REPORT ===");

            let email_m = email_metrics.lock().unwrap().clone();
            let payment_m = payment_metrics.lock().unwrap().clone();
            let report_m = report_metrics.lock().unwrap().clone();

            for (name, metrics) in [
                ("Email", email_m),
                ("Payment", payment_m),
                ("Report", report_m),
            ] {
                if !metrics.is_empty() {
                    println!("{} Jobs:", name);
                    for (key, value) in metrics {
                        println!("  {}: {}", key, value);
                    }
                }
            }
            println!("========================\n");
        }
    });

    // Start worker
    println!("🚀 Starting worker with custom processors...");

    // Run both the worker and metrics reporting
    tokio::select! {
        result = queue.start_worker() => {
            println!("Worker finished: {:?}", result);
            result
        }
        _ = metrics_task => {
            println!("Metrics task finished");
            Ok(())
        }
    }
}
