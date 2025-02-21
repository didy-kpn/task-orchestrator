# Task Orchestrator 🚀

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A robust, type-safe asynchronous task scheduling library with event-driven architecture, built on Tokio.

## Features ✨

- 🚦 **Dual execution modes**: Background and Event-driven tasks
- 📡 **Configurable event channels** with capacity control
- ⚡ **Asynchronous execution** powered by Tokio
- 🔒 **Type-safe event system** with compile-time guarantees
- 🔄 **Task lifecycle management** (initialize, execute, shutdown)
- 🧩 **Extensible architecture** with trait-based design
- 🛡️ **Error handling** with custom SchedulerError type
- 📊 **Task status tracking** (Idle, Running, Stopped, Failed)

## Installation 📦

Add this to your `Cargo.toml`:

```toml
[dependencies]
task-orchestrator = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
```

## Quick Start 🚀

```rust
use task_orchestrator::{EventBus, Scheduler, EventType, Executable, BackgroundTask, EventDrivenTask};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum MyEvent {
    ProcessData,
    ShutdownSignal,
}

impl std::fmt::Display for MyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MyEvent::ProcessData => write!(f, "ProcessData"),
            MyEvent::ShutdownSignal => write!(f, "ShutdownSignal"),
        }
    }
}

impl EventType for MyEvent {}

struct BackgroundProcessor {
    name: String,
    status: ExecutionStatus,
    status_inner: Arc<Mutex<ExecutionStatus>>,
}

#[async_trait::async_trait]
impl Executable for BackgroundProcessor {
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
impl BackgroundTask for BackgroundProcessor {
    async fn execute(&mut self) -> Result<(), SchedulerError> {
        // Your background processing logic here
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

#[tokio::main]
async fn main() {
    let event_bus = EventBus::new(vec![
        (MyEvent::ProcessData, ChannelConfig {
            capacity: 100,
            description: "Data processing channel".to_string(),
        }),
        (MyEvent::ShutdownSignal, ChannelConfig {
            capacity: 10,
            description: "Shutdown channel".to_string(),
        }),
    ]);
    
    let mut scheduler = Scheduler::new(event_bus);
    
    // Register background task
    let bg_task = BackgroundProcessor {
        name: "DataProcessor".to_string(),
        status: ExecutionStatus::Idle,
        status_inner: Arc::new(Mutex::new(ExecutionStatus::Idle)),
    };
    scheduler.register_background_task(Box::new(bg_task));
    
    // Start the scheduler
    scheduler.start().await.unwrap();
    
    // Shutdown after some time
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    scheduler.shutdown().await.unwrap();
}
```

## License 📄

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
