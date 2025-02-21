use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use task_orchestrator::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum BenchEvent {
    HighPriority,
}

impl std::fmt::Display for BenchEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchEvent::HighPriority => write!(f, "HighPriority"),
        }
    }
}

impl EventType for BenchEvent {}

struct BackgroundBenchmarkTask {
    name: String,
    status: ExecutionStatus,
    status_inner: Arc<Mutex<ExecutionStatus>>,
}

#[async_trait::async_trait]
impl Executable for BackgroundBenchmarkTask {
    fn name(&self) -> &str {
        &self.name
    }

    fn status(&self) -> ExecutionStatus {
        self.status
    }

    async fn initialize(&mut self) -> Result<(), SchedulerError> {
        *self.status_inner.lock().await = ExecutionStatus::Running;
        self.status = ExecutionStatus::Running;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), SchedulerError> {
        *self.status_inner.lock().await = ExecutionStatus::Stopped;
        self.status = ExecutionStatus::Stopped;
        Ok(())
    }
}

#[async_trait::async_trait]
impl BackgroundTask for BackgroundBenchmarkTask {
    async fn execute(&mut self) -> Result<(), SchedulerError> {
        tokio::time::sleep(Duration::from_micros(10)).await;
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn BackgroundTask> {
        Box::new(Self {
            name: self.name.clone(),
            status: self.status,
            status_inner: self.status_inner.clone(),
        })
    }
}

struct EventDrivenBenchmarkTask {
    name: String,
    status: ExecutionStatus,
    status_inner: Arc<Mutex<ExecutionStatus>>,
    event: BenchEvent,
    received_events: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl Executable for EventDrivenBenchmarkTask {
    fn name(&self) -> &str {
        &self.name
    }

    fn status(&self) -> ExecutionStatus {
        self.status
    }

    async fn initialize(&mut self) -> Result<(), SchedulerError> {
        *self.status_inner.lock().await = ExecutionStatus::Running;
        self.status = ExecutionStatus::Running;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), SchedulerError> {
        *self.status_inner.lock().await = ExecutionStatus::Stopped;
        self.status = ExecutionStatus::Stopped;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventDrivenTask<BenchEvent> for EventDrivenBenchmarkTask {
    fn subscribed_event(&self) -> &BenchEvent {
        &self.event
    }

    async fn handle_event(&mut self, event: String) -> Result<(), SchedulerError> {
        self.received_events.lock().await.push(event);
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn EventDrivenTask<BenchEvent>> {
        Box::new(Self {
            name: self.name.clone(),
            status: self.status,
            status_inner: self.status_inner.clone(),
            event: self.event.clone(),
            received_events: self.received_events.clone(),
        })
    }
}

fn scheduler_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("schedule_1000_background_tasks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let event_bus = EventBus::<BenchEvent>::new(vec![]);
                let mut scheduler = Scheduler::new(event_bus);

                for i in 0..1000 {
                    let task = BackgroundBenchmarkTask {
                        name: format!("bg_task_{}", i),
                        status: ExecutionStatus::Idle,
                        status_inner: Arc::new(Mutex::new(ExecutionStatus::Idle)),
                    };
                    scheduler.register_background_task(Box::new(task));
                }

                scheduler.start().await.unwrap();
                tokio::time::sleep(Duration::from_millis(100)).await;
                scheduler.shutdown().await.unwrap();
            });
        })
    });

    c.bench_function("handle_10000_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let event_bus = EventBus::new(vec![(
                    BenchEvent::HighPriority,
                    ChannelConfig {
                        capacity: 10000,
                        description: "Benchmark channel".to_string(),
                    },
                )]);

                let mut scheduler = Scheduler::new(event_bus);
                let sender = scheduler
                    .event_bus()
                    .clone_sender(&BenchEvent::HighPriority)
                    .unwrap();

                // Create event-driven task
                let task = EventDrivenBenchmarkTask {
                    name: "event_handler".to_string(),
                    status: ExecutionStatus::Idle,
                    status_inner: Arc::new(Mutex::new(ExecutionStatus::Idle)),
                    event: BenchEvent::HighPriority,
                    received_events: Arc::new(Mutex::new(Vec::new())),
                };
                scheduler.register_event_driven_task(Box::new(task));

                scheduler.start().await.unwrap();

                // Send events
                for i in 0..10000 {
                    sender.send(format!("event_{}", i)).unwrap();
                }

                // Wait for processing
                tokio::time::sleep(Duration::from_millis(500)).await;
                scheduler.shutdown().await.unwrap();
            });
        })
    });
}

criterion_group!(benches, scheduler_benchmark);
criterion_main!(benches);
